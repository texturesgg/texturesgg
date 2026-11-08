# tgg-account

Signing in to textures.gg, shared by tgg-studio and the `tgg` command line so
both use one login and one credentials file.

- **Device login** (OAuth device flow): `Site::begin_device_login` hands out a
  code to confirm on the site, `Site::poll_device_login` asks once whether it
  was confirmed, and `Site::login` does both, blocking, for a command line.
- **Credentials**: `credentials.json` in the textures.gg config folder, one
  token per API base URL, written atomically and readable only by the user.
- **Requests as the user**: `Site::get`, `post` and `exists` send the saved
  token as `Authorization: Bearer`; `Site::net` hands it to a `tgg_mod::HttpNet`.

Plain blocking Rust with no UI dependencies. Errors are typed and their
`Display` reads well to a player. The API is in `docs/mod-distribution.md`
under "Sign-in".
