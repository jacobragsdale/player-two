//! The hub's one SQLite file, which no other process may open while the hub runs.

use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use anyhow::{Context, bail};
use rusqlite::Connection;

/// Each entry brings the schema from the version before it to its own; the first starts from an
/// empty file. `PRAGMA user_version` holds how many have been applied.
const MIGRATIONS: &[&str] = &["
    CREATE TABLE people (
        name TEXT PRIMARY KEY,
        token_sha256 TEXT,
        joined INTEGER NOT NULL,
        last_seen INTEGER
    );
    CREATE TABLE channels (
        name TEXT PRIMARY KEY,
        created_by TEXT NOT NULL,
        created INTEGER NOT NULL
    );
    CREATE TABLE members (
        channel TEXT NOT NULL REFERENCES channels (name),
        name TEXT NOT NULL,
        PRIMARY KEY (channel, name)
    );
    CREATE TABLE events (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        chat TEXT NOT NULL,
        thread INTEGER,
        author TEXT NOT NULL,
        kind TEXT NOT NULL,
        body TEXT NOT NULL,
        ts INTEGER NOT NULL
    );
    CREATE INDEX events_by_chat ON events (chat, id);
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
    connection: Arc<Mutex<Connection>>,
}

impl Db {
    /// Opens the file, takes it for this process alone, checks it, and brings its schema up to
    /// date. A damaged file, or one another hub holds, is refused.
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let connection = Connection::open(path)
            .with_context(|| format!("cannot open the database at {}", path.display()))?;
        prepare(&connection).with_context(|| {
            format!(
                "the database at {} cannot be used; restore last night's copy from the backups \
                 folder, or stop the other hub using it",
                path.display()
            )
        })?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    /// Runs `work` with the connection, on a thread where waiting on the disk blocks nothing else.
    pub async fn call<T, F>(&self, work: F) -> anyhow::Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection) -> anyhow::Result<T> + Send + 'static,
    {
        let connection = Arc::clone(&self.connection);
        tokio::task::spawn_blocking(move || work(&mut lock(&connection)))
            .await
            .context("a database task stopped before it finished")?
    }
}

/// A panic while the connection was held leaves it as SQLite left it: every write is a
/// transaction, so taking it back is safe.
fn lock(connection: &Mutex<Connection>) -> MutexGuard<'_, Connection> {
    connection.lock().unwrap_or_else(PoisonError::into_inner)
}

fn prepare(connection: &Connection) -> anyhow::Result<()> {
    // Nothing else should ever hold the file, so waiting for it would only hide a second hub.
    connection.busy_timeout(Duration::ZERO)?;
    // Exclusive before WAL: SQLite then keeps WAL's index in this process's memory instead of in
    // a shared file, which a network file system cannot be trusted with.
    let locking: String =
        connection.query_row("PRAGMA locking_mode = EXCLUSIVE", [], |row| row.get(0))?;
    if locking != "exclusive" {
        bail!("SQLite would not lock the file for this process alone");
    }
    let journal: String =
        connection.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))?;
    if journal != "wal" {
        bail!("SQLite would not write ahead (it chose {journal})");
    }
    connection.execute_batch("PRAGMA synchronous = FULL; PRAGMA foreign_keys = ON;")?;
    let check: String = connection.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
    if check != "ok" {
        bail!("SQLite's check of the file found it damaged: {check}");
    }
    migrate(connection)
}

fn migrate(connection: &Connection) -> anyhow::Result<()> {
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let done = usize::try_from(version).context("the schema's version is negative")?;
    if done > MIGRATIONS.len() {
        bail!("the database is from a newer hub (schema {done}); run that hub's version");
    }
    for (index, migration) in MIGRATIONS.iter().enumerate().skip(done) {
        connection.execute_batch(&format!(
            "BEGIN; {migration} PRAGMA user_version = {}; COMMIT;",
            index + 1
        ))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{Db, MIGRATIONS};

    #[tokio::test]
    async fn a_new_file_gets_the_schema() {
        let folder = tempfile::tempdir().expect("a temporary folder");
        let db = Db::open(&folder.path().join("hub.db")).expect("a new database opens");
        let (version, tables) = db
            .call(|connection| {
                let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
                let tables: i64 = connection.query_row(
                    "SELECT count(*) FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
                    [],
                    |row| row.get(0),
                )?;
                Ok((version, tables))
            })
            .await
            .expect("the schema can be read");
        assert_eq!(usize::try_from(version).ok(), Some(MIGRATIONS.len()));
        assert_eq!(tables, 5);
    }

    #[test]
    fn a_damaged_file_is_refused() {
        let folder = tempfile::tempdir().expect("a temporary folder");
        let garbage = folder.path().join("garbage.db");
        fs::write(&garbage, vec![0x5a; 8192]).expect("a file can be written");
        assert!(Db::open(&garbage).is_err());

        // A real database with one of its pages overwritten.
        let path = folder.path().join("hub.db");
        {
            let connection = rusqlite::Connection::open(&path).expect("opens");
            connection
                .execute_batch(
                    "PRAGMA page_size = 4096; CREATE TABLE t (x TEXT);
                     WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 2000)
                     INSERT INTO t SELECT printf('%0200d', i) FROM n;",
                )
                .expect("filled");
        }
        let mut bytes = fs::read(&path).expect("read back");
        bytes[3 * 4096..4 * 4096].fill(0xff);
        fs::write(&path, bytes).expect("damaged");
        let refused = Db::open(&path).err().map(|error| format!("{error:#}"));
        assert!(refused.is_some_and(|error| error.contains("restore last night's copy")));
    }

    #[test]
    fn a_second_hub_on_the_same_file_is_refused() {
        let folder = tempfile::tempdir().expect("a temporary folder");
        let path = folder.path().join("hub.db");
        let _first = Db::open(&path).expect("the first hub opens it");
        assert!(Db::open(&path).is_err(), "the second hub is refused");
    }
}
