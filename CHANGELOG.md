# webthing Changelog

## [Unreleased]
### Changed
- Now using 2018 edition of Rust.
- Updated to modern version of actix-web.
- Server is now created and started with `.start()`; `.create()` is no longer present.
- Server no longer takes an optional router. Instead, an optional `configure` closure can be passed to `.start()`, which will be used via `App.configure(<your closure>)`.

## [0.12.3] - 2020-05-04
### Changed
- Invalid POST requests to action resources now generate an error status.

## [0.12.2] - 2020-03-27
### Changed
- Updated dependencies.

## [0.12.1] - 2019-11-07
### Changed
- Fixed deprecations.

## [0.12.0] - 2019-07-12
### Changed
- Things now use `title` rather than `name`.
- Things now require a unique ID in the form of a URI.
### Added
- Ability to set a base URL path on server.
- Support for `id`, `base`, `security`, and `securityDefinitions` keys in thing description.

## [0.11.0] - 2019-01-16
### Changed
- `WebThingServer::new()` can now take a configuration function which can add additional API routes.
### Fixed
- Properties could not include a custom `links` array at initialization.

## [0.10.3] - 2018-12-18
### Fixed
- SSL feature compilation.

## [0.10.2] - 2018-12-18
### Changed
- SSL is now an optional feature.

## [0.10.1] - 2018-12-13
### Changed
- Properties, actions, and events should now use `title` rather than `label`.

## [0.10.0] - 2018-11-30
### Changed
- Property, Action, and Event description now use `links` rather than `href`. - [Spec PR](https://github.com/mozilla-iot/wot/pull/119)

[Unreleased]: https://github.com/mozilla-iot/webthing-rust/compare/v0.12.3...HEAD
[0.12.3]: https://github.com/mozilla-iot/webthing-rust/compare/v0.12.2...v0.12.3
[0.12.2]: https://github.com/mozilla-iot/webthing-rust/compare/v0.12.1...v0.12.2
[0.12.1]: https://github.com/mozilla-iot/webthing-rust/compare/v0.12.0...v0.12.1
[0.12.0]: https://github.com/mozilla-iot/webthing-rust/compare/v0.11.0...v0.12.0
[0.11.0]: https://github.com/mozilla-iot/webthing-rust/compare/v0.10.3...v0.11.0
[0.10.3]: https://github.com/mozilla-iot/webthing-rust/compare/v0.10.2...v0.10.3
[0.10.2]: https://github.com/mozilla-iot/webthing-rust/compare/v0.10.1...v0.10.2
[0.10.1]: https://github.com/mozilla-iot/webthing-rust/compare/v0.10.0...v0.10.1
[0.10.0]: https://github.com/mozilla-iot/webthing-rust/compare/v0.9.3...v0.10.0
