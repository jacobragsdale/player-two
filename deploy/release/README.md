# The installer the hub hands out

The release pipeline copies the Windows installer and its updater signature here before it builds
the hub's image, as `player-two-setup.exe` and `player-two-setup.exe.sig`. The image carries them
in `/app/release`, and the hub serves them at `/download` and `/update`. Nothing here but this file
is committed.
