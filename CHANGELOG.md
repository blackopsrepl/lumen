# Changelog

All notable changes to this project will be documented in this file. See [commit-and-tag-version](https://github.com/absolute-version/commit-and-tag-version) for commit guidelines.

## [0.14.3](https://github.com/blackopsrepl/lumen/compare/v0.14.2...v0.14.3) (2026-10-04)


### Bug Fixes

* **build:** include QElapsedTimer and QRect explicitly ([d8c8901](https://github.com/blackopsrepl/lumen/commit/d8c89019aa21f7e2e1cda854692152992fff1297))

## [0.14.2](https://github.com/blackopsrepl/lumen/compare/v0.14.1...v0.14.2) (2026-10-04)


### Bug Fixes

* **accessibility:** accessibilityReady now means a walk would return a tree ([848d1f2](https://github.com/blackopsrepl/lumen/commit/848d1f22bd0aefc112ab5fb6017e5e736cde2eb8))

## [0.14.1](https://github.com/blackopsrepl/lumen/compare/v0.14.0...v0.14.1) (2026-10-04)


### Bug Fixes

* **release:** take the notes from the changelog, not generated notes ([8f18cf7](https://github.com/blackopsrepl/lumen/commit/8f18cf7470fe948c30ae352a3fa770794129afbc))
* **viewer:** every session opens fitted, not just the first ([0e331fa](https://github.com/blackopsrepl/lumen/commit/0e331fa535e9c2838538d4b0c3ede960bac20e1d))
* **viewer:** notes send again, and the zoom the old viewer had ([c2272ab](https://github.com/blackopsrepl/lumen/commit/c2272abd1408a6d5508a7d711735336aae34b827))

## [0.14.0](https://github.com/blackopsrepl/lumen/compare/v0.13.7...v0.14.0) (2026-10-04)


### Features

* **compositor:** C++ Wayland compositor with windowless output ([b075516](https://github.com/blackopsrepl/lumen/commit/b075516036431f3181a0d3bb8fb2cae49c6f22e4))
* **viewer:** icon-only controls, and one place that styles them ([f63b957](https://github.com/blackopsrepl/lumen/commit/f63b9576677462bfe16cc0731c57c1cb1bc4b05a))
* **viewer:** render the session, and let Lumen host itself ([2d007d8](https://github.com/blackopsrepl/lumen/commit/2d007d8942e50916e3c6afc0edb8e0f7f7e3c5c6))
* **viewer:** restore the human-to-agent feedback loop ([1eb8dcc](https://github.com/blackopsrepl/lumen/commit/1eb8dcca263b791dd5f1910df8be52d166eb9c70))


### Bug Fixes

* **ci:** drop sudo — act runs the job as root and has no sudo ([4fdb181](https://github.com/blackopsrepl/lumen/commit/4fdb1819ceb12e41b67b846e60e66539b262bc24))
* **ci:** install Qt from aqtinstall, because noble's package is broken ([9042ee7](https://github.com/blackopsrepl/lumen/commit/9042ee7d2c2d241519d4a96bfc87f783bca55ab8))
* **ci:** run the Forgejo gate on the C++ runner that exists ([1182768](https://github.com/blackopsrepl/lumen/commit/1182768f0abcce98ee06ed69834810c32170d8ec))
* **ci:** trigger on any branch, not a named one ([cd77b97](https://github.com/blackopsrepl/lumen/commit/cd77b973c4ac7ca97fc988ad05a3365ea37e8acb))
* **compositor:** capture frames via bufferCommitted and unblock client drawing ([d3ab929](https://github.com/blackopsrepl/lumen/commit/d3ab929f4fc0782a6f397a130d6c7343a98d01d6))
* **install:** enable linger so the daemon really does survive logout ([16175f9](https://github.com/blackopsrepl/lumen/commit/16175f95b98336b2fc643eef67a42668321c2892))
* **session:** stream any client, and drop the viewer's own session form ([9cf3c52](https://github.com/blackopsrepl/lumen/commit/9cf3c5272d982fbd1ad8c60b47db6e62889790b5))
