# Security Policy

## Reporting a vulnerability

Email **security@textures.gg** with what you found and how to reproduce it.
Do not open a public issue or pull request for a suspected vulnerability.

The parsers read untrusted files, so a DAT or disc image that makes a crate
or the desktop app read out of bounds, loop without end, allocate without
limit, or panic is in scope. Include, where you can:

- the affected crate and version;
- a file or the bytes that trigger the problem, built by hand rather than
  taken from the game;
- what an attacker gains from it.

Fixes land in the latest published version of each crate.

The website lives in [texturesgg/site](https://github.com/texturesgg/site),
and the same address reaches its maintainers.
