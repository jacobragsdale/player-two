//! The hub's Postgres database, which nothing else uses. The hub keeps one connection, so that
//! events are written one at a time and their ids come in the order they were written.

use std::future::Future;
use std::sync::Arc;

use anyhow::{Context, bail};
use tokio::sync::{Mutex, MutexGuard};
use tokio_postgres::{Client, Config, NoTls};

/// Each entry brings the schema from the version before it to its own; the first starts from an
/// empty database. The one row of `schema_version` holds how many have been applied.
const MIGRATIONS: &[&str] = &["
    CREATE TABLE people (
        name TEXT PRIMARY KEY,
        token_sha256 TEXT,
        joined BIGINT NOT NULL,
        last_seen BIGINT
    );
    CREATE TABLE channels (
        name TEXT PRIMARY KEY,
        created_by TEXT NOT NULL,
        created BIGINT NOT NULL
    );
    CREATE TABLE members (
        channel TEXT NOT NULL REFERENCES channels (name),
        name TEXT NOT NULL,
        PRIMARY KEY (channel, name)
    );
    CREATE TABLE events (
        id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
        chat TEXT NOT NULL,
        thread BIGINT,
        author TEXT NOT NULL,
        kind TEXT NOT NULL,
        body TEXT NOT NULL,
        ts BIGINT NOT NULL
    );
    CREATE INDEX events_by_chat ON events (chat, id);
    CREATE INDEX events_by_thread ON events (thread);
    CREATE TABLE places (
        owner TEXT NOT NULL,
        name TEXT NOT NULL,
        kind TEXT NOT NULL,
        PRIMARY KEY (owner, name)
    );
"];

/// The database, shared by every request. One connection, used by one request at a time.
#[derive(Clone)]
pub struct Db {
    client: Arc<Mutex<Client>>,
}

impl Db {
    /// Connects, and brings the schema up to date. The future given back drives the connection,
    /// and resolves with the reason when it is lost; the hub stops then.
    pub async fn connect(
        config: &Config,
    ) -> anyhow::Result<(Self, impl Future<Output = anyhow::Error> + Send + use<>)> {
        let (mut client, connection) = config
            .connect(NoTls)
            .await
            .context("cannot connect to the database at PLAYER_TWO_DATABASE_URL")?;
        let connection = async move {
            match connection.await {
                Ok(()) => anyhow::anyhow!("the database closed the connection"),
                Err(error) => anyhow::Error::from(error).context("lost the database"),
            }
        };
        // The connection must be driven while the schema is brought up to date.
        let driven = tokio::spawn(connection);
        migrate(&mut client).await?;
        let lost = async move {
            driven
                .await
                .unwrap_or_else(|error| anyhow::anyhow!("the database's task stopped: {error}"))
        };
        Ok((
            Self {
                client: Arc::new(Mutex::new(client)),
            },
            lost,
        ))
    }

    /// Waits for the connection, and holds it until the guard is dropped.
    pub async fn lock(&self) -> MutexGuard<'_, Client> {
        self.client.lock().await
    }
}

async fn migrate(client: &mut Client) -> anyhow::Result<()> {
    client
        .batch_execute(
            "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);
             INSERT INTO schema_version (version)
                 SELECT 0 WHERE NOT EXISTS (SELECT 1 FROM schema_version);",
        )
        .await?;
    let version: i32 = client
        .query_one("SELECT version FROM schema_version", &[])
        .await?
        .get(0);
    let done = usize::try_from(version).context("the schema's version is negative")?;
    if done > MIGRATIONS.len() {
        bail!("the database is from a newer hub (schema {done}); run that hub's version");
    }
    for (index, migration) in MIGRATIONS.iter().enumerate().skip(done) {
        let transaction = client.transaction().await?;
        transaction.batch_execute(migration).await?;
        let version = i32::try_from(index + 1)?;
        transaction
            .execute("UPDATE schema_version SET version = $1", &[&version])
            .await?;
        transaction.commit().await?;
    }
    Ok(())
}

/// A database of its own for one test, made in the Postgres that `PLAYER_TWO_TEST_DATABASE_URL`
/// names, and dropped with everything in it when this is dropped.
pub struct TestDatabase {
    server: Config,
    name: String,
}

/// What a test run without a database to use says.
const NO_TEST_DATABASE: &str = "the tests need a Postgres to make databases in, named by \
    PLAYER_TWO_TEST_DATABASE_URL; for example, run \
    `docker run -d --name player-two-test-db -e POSTGRES_PASSWORD=postgres -p 127.0.0.1:55432:5432 postgres:17-alpine` \
    and set PLAYER_TWO_TEST_DATABASE_URL=postgres://postgres:postgres@127.0.0.1:55432/postgres";

impl TestDatabase {
    /// Makes a new, empty database. It fails, saying how to start one, without a Postgres to
    /// make it in.
    pub async fn new() -> anyhow::Result<Self> {
        let url = std::env::var("PLAYER_TWO_TEST_DATABASE_URL").context(NO_TEST_DATABASE)?;
        let server: Config = url
            .parse()
            .context("PLAYER_TWO_TEST_DATABASE_URL is not a postgres:// address")?;
        let mut bytes = [0_u8; 8];
        getrandom::fill(&mut bytes).map_err(|error| anyhow::anyhow!("no randomness: {error}"))?;
        let name = format!("player_two_test_{}", u64::from_le_bytes(bytes));
        let (client, connection) = server.connect(NoTls).await.context(NO_TEST_DATABASE)?;
        let driven = tokio::spawn(connection);
        client
            .batch_execute(&format!("CREATE DATABASE {name}"))
            .await?;
        drop(client);
        let _ = driven.await;
        Ok(Self { server, name })
    }

    /// How to reach the database.
    #[must_use]
    pub fn config(&self) -> Config {
        let mut config = self.server.clone();
        config.dbname(&self.name);
        config
    }
}

impl Drop for TestDatabase {
    fn drop(&mut self) {
        let (server, name) = (self.server.clone(), self.name.clone());
        // Drop runs where nothing may be awaited, so the database is dropped on a thread of its
        // own, with a runtime of its own.
        let dropped = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            runtime.block_on(async move {
                let (client, connection) = server.connect(NoTls).await?;
                tokio::spawn(connection);
                client
                    .batch_execute(&format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)"))
                    .await?;
                anyhow::Ok(())
            })
        })
        .join();
        if !matches!(dropped, Ok(Ok(()))) {
            eprintln!("warning: the test database {} was not dropped", self.name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Db, MIGRATIONS, TestDatabase};

    #[tokio::test]
    async fn a_new_database_gets_the_schema_once() {
        let scratch = TestDatabase::new().await.expect("a test database");
        let (db, _) = Db::connect(&scratch.config())
            .await
            .expect("a new database is set up");
        let (version, tables): (i32, i64) = {
            let client = db.lock().await;
            let version = client
                .query_one("SELECT version FROM schema_version", &[])
                .await
                .expect("the version reads")
                .get(0);
            let tables = client
                .query_one(
                    "SELECT count(*) FROM information_schema.tables WHERE table_schema = 'public'",
                    &[],
                )
                .await
                .expect("the tables are listed")
                .get(0);
            (version, tables)
        };
        assert_eq!(usize::try_from(version).ok(), Some(MIGRATIONS.len()));
        assert_eq!(tables, 6, "five tables and the schema's version");
        // Connecting again finds the schema up to date and changes nothing.
        let (again, _) = Db::connect(&scratch.config())
            .await
            .expect("an up-to-date database opens");
        let count: i64 = again
            .lock()
            .await
            .query_one("SELECT count(*) FROM schema_version", &[])
            .await
            .expect("the version reads")
            .get(0);
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn losing_the_connection_is_said() {
        let scratch = TestDatabase::new().await.expect("a test database");
        let (db, lost) = Db::connect(&scratch.config()).await.expect("set up");
        let pid: i32 = db
            .lock()
            .await
            .query_one("SELECT pg_backend_pid()", &[])
            .await
            .expect("the connection says who it is")
            .get(0);
        let (other, _) = Db::connect(&scratch.config()).await.expect("a second");
        other
            .lock()
            .await
            .execute("SELECT pg_terminate_backend($1)", &[&pid])
            .await
            .expect("the first connection is ended");
        let error = tokio::time::timeout(std::time::Duration::from_secs(10), lost)
            .await
            .expect("the loss is noticed");
        assert!(format!("{error:#}").contains("database"));
    }

    #[tokio::test]
    async fn a_database_from_a_newer_hub_is_refused() {
        let scratch = TestDatabase::new().await.expect("a test database");
        let (db, _) = Db::connect(&scratch.config()).await.expect("set up");
        db.lock()
            .await
            .execute("UPDATE schema_version SET version = 99", &[])
            .await
            .expect("the version changes");
        let refused = Db::connect(&scratch.config())
            .await
            .err()
            .map(|error| format!("{error:#}"));
        assert!(refused.is_some_and(|error| error.contains("newer hub")));
    }
}
