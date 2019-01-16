# webthing Changelog

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

[Unreleased]: https://github.com/mozilla-iot/webthing-rust/compare/v0.11.0...HEAD
[0.11.0]: https://github.com/mozilla-iot/webthing-rust/compare/v0.10.3...v0.11.0
[0.10.3]: https://github.com/mozilla-iot/webthing-rust/compare/v0.10.2...v0.10.3
[0.10.2]: https://github.com/mozilla-iot/webthing-rust/compare/v0.10.1...v0.10.2
[0.10.1]: https://github.com/mozilla-iot/webthing-rust/compare/v0.10.0...v0.10.1
[0.10.0]: https://github.com/mozilla-iot/webthing-rust/compare/v0.9.3...v0.10.0
