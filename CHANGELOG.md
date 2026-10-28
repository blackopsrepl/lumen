# Changelog

All notable changes to this project will be documented in this file. See [commit-and-tag-version](https://github.com/absolute-version/commit-and-tag-version) for commit guidelines.

## [0.15.2](https://github.com/blackopsrepl/lumen/compare/v0.15.1...v0.15.2) (2026-10-28)


### Bug Fixes

* **ci:** stop a tag's two ci runs from cancelling each other ([67f7e4d](https://github.com/blackopsrepl/lumen/commit/67f7e4d9ef697729d511e1782193716420201b36))


## [0.15.1](https://github.com/blackopsrepl/lumen/compare/v0.15.0...v0.15.1) (2026-10-28)


### Bug Fixes

* **build:** configure and compile without warnings ([870dce2](https://github.com/blackopsrepl/lumen/commit/870dce2a01f2aa7c2eda82727608907aed50bef4))
* **version:** report the project version, not a stale literal ([c9fde4b](https://github.com/blackopsrepl/lumen/commit/c9fde4b20240a7f2216b8138e314b1ca27416a93))


## [0.15.0](https://github.com/blackopsrepl/lumen/compare/v0.14.4...v0.15.0) (2026-10-27)


### Features

* **compositor:** host Quickshell panels and their popups ([4f971c2](https://github.com/blackopsrepl/lumen/commit/4f971c207159291dfb188e4a3b551084ae8af586))


### Bug Fixes

* **agent:** make input that lands nowhere an error ([5dce087](https://github.com/blackopsrepl/lumen/commit/5dce087f9d1c720b022025c48b9c013f068d6f62))
* **cli:** honour the published agent socket ([0f0fac1](https://github.com/blackopsrepl/lumen/commit/0f0fac1ac75056dfa9e85a3c910b643cf4cbac6a))
* **compositor:** attribute surfaces through the session's environment ([27cf323](https://github.com/blackopsrepl/lumen/commit/27cf32330e53faefd6e1dc9fdac421ede0bd08a2))
* **compositor:** keep every surface a session maps on screen ([3184420](https://github.com/blackopsrepl/lumen/commit/31844203efa00fc108808a5c4bf052165f68c632))
* **daemon:** run the compositor on the offscreen platform ([9c53cf0](https://github.com/blackopsrepl/lumen/commit/9c53cf05de1b3bc1caa28776e6d4c5187e87a910))
* **viewer:** show the session an agent starts ([0c011e8](https://github.com/blackopsrepl/lumen/commit/0c011e84af38f4668679deecfb9cc5527ac51a71))


## [0.14.4](https://github.com/blackopsrepl/lumen/compare/v0.14.3...v0.14.4) (2026-10-04)


### Bug Fixes

* **annotation:** make the note workflow work end to end ([1785777](https://github.com/blackopsrepl/lumen/commit/17857772eeadc2a7ffbc8be889b90a951ee00268))
* **annotation:** the overlay needs hover, not just a press ([41b8454](https://github.com/blackopsrepl/lumen/commit/41b845499c4dd9fe248235c4b14a4699e86246b1))

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
