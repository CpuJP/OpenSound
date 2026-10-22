# Security policy

## Supported versions

Only the latest release and the `main` branch receive fixes.

## Reporting a vulnerability

Please **do not** open a public issue for security problems. Report them privately through [GitHub security advisories](https://github.com/CpuJP/OpenSound/security/advisories/new).

Include a description of the issue, steps to reproduce it and its impact. You will get an answer as soon as possible. Once a fix is available, the issue will be disclosed publicly with credit to the reporter, unless you prefer to stay anonymous.

## Scope

OpenSound runs entirely as your user. It needs no root, opens no network sockets and talks only to the session D-Bus and to the paired Bluetooth device. Any other application in your user session can call the D-Bus API, which is the same trust level as the system's own Bluetooth and audio settings.
