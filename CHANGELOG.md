# Changelog

All notable changes to this project will be documented in this file.

## [0.34.1] - 2023-06-02

### Bug Fixes

- Auth logic ([#224](https://github.com/sigoden/dufs/issues/224))
- Allow all cors headers and methods ([#225](https://github.com/sigoden/dufs/issues/225))

### Refactor

- Ui checkAuth ([#226](https://github.com/sigoden/dufs/issues/226))

## [0.34.0] - 2023-06-01

### Bug Fixes

- URL-encoded filename when downloading in safari ([#203](https://github.com/sigoden/dufs/issues/203))
- Ui path table show move action ([#219](https://github.com/sigoden/dufs/issues/219))
- Ui set default max uploading to 1 ([#220](https://github.com/sigoden/dufs/issues/220))

### Features

- Webui editing support multiple encodings ([#197](https://github.com/sigoden/dufs/issues/197))
- Add timestamp metadata to generated zip file ([#204](https://github.com/sigoden/dufs/issues/204))
- Show precise file size with decimal ([#210](https://github.com/sigoden/dufs/issues/210))
- [**breaking**] New auth ([#218](https://github.com/sigoden/dufs/issues/218))

### Refactor

- Cli positional rename root => SERVE_PATH([#215](https://github.com/sigoden/dufs/issues/215))

## [0.33.0] - 2023-03-17

### Bug Fixes

- Cors allow-request-header add content-type ([#184](https://github.com/sigoden/dufs/issues/184))
- Hidden don't works on some files ([#188](https://github.com/sigoden/dufs/issues/188))
- Basic auth sometimes does not work ([#194](https://github.com/sigoden/dufs/issues/194))

### Features

- Guess plain text encoding then set content-type charset ([#186](https://github.com/sigoden/dufs/issues/186))

### Refactor

- Improve error handle ([#195](https://github.com/sigoden/dufs/issues/195))

## [0.32.0] - 2023-02-22

### Bug Fixes

- Set the STOPSIGNAL to SIGINT for Dockerfile
- Remove Method::Options auth check ([#168](https://github.com/sigoden/dufs/issues/168))
- Clear search input also clear query ([#178](https://github.com/sigoden/dufs/issues/178))

### Features

- [**breaking**] Add option --allow-archive ([#152](https://github.com/sigoden/dufs/issues/152))
- Use env var for args ([#170](https://github.com/sigoden/dufs/issues/170))
- Hiding only directories instead of files ([#175](https://github.com/sigoden/dufs/issues/175))
- API to search and list directories ([#177](https://github.com/sigoden/dufs/issues/177))
- Support edit files ([#179](https://github.com/sigoden/dufs/issues/179))
- Support new file ([#180](https://github.com/sigoden/dufs/issues/180))
- Ui improves the login experience ([#182](https://github.com/sigoden/dufs/issues/182))

## [0.31.0] - 2022-11-11

### Bug Fixes

- Auth not works with --path-prefix ([#138](https://github.com/sigoden/dufs/issues/138))
- Don't search on empty query string ([#140](https://github.com/sigoden/dufs/issues/140))
- Status code for MKCOL on existing resource ([#142](https://github.com/sigoden/dufs/issues/142))
- Panic on PROPFIND // ([#144](https://github.com/sigoden/dufs/issues/144))

### Features

- Support unix sockets ([#145](https://github.com/sigoden/dufs/issues/145))

## [0.30.0] - 2022-09-09

### Bug Fixes

- Hide path by ext name ([#126](https://github.com/sigoden/dufs/issues/126))

### Features

- Support sort by name, mtime, size ([#128](https://github.com/sigoden/dufs/issues/128))
- Add --assets options to override assets ([#134](https://github.com/sigoden/dufs/issues/134))

## [0.29.0] - 2022-08-03

### Bug Fixes

- Table row hover highlighting in dark mode ([#122](https://github.com/sigoden/dufs/issues/122))

### Features

- Support ecdsa tls cert ([#119](https://github.com/sigoden/dufs/issues/119))

## [0.28.0] - 2022-08-01

### Bug Fixes

- File path contains special characters ([#114](https://github.com/sigoden/dufs/issues/114))

### Features

- Add table row hover ([#115](https://github.com/sigoden/dufs/issues/115))
- Support customize http log format ([#116](https://github.com/sigoden/dufs/issues/116))

## [0.27.0] - 2022-07-25

### Features

- Improve hidden to support glob ([#108](https://github.com/sigoden/dufs/issues/108))
- Adjust digest auth timeout to 1day ([#110](https://github.com/sigoden/dufs/issues/110))

## [0.26.0] - 2022-07-11

### Bug Fixes

- Cors headers ([#100](https://github.com/sigoden/dufs/issues/100))

### Features

- Make --path-prefix works on serving single file ([#102](https://github.com/sigoden/dufs/issues/102))

## [0.25.0] - 2022-07-06

### Features

- Ui supports creating folder ([#91](https://github.com/sigoden/dufs/issues/91))
- Ui supports move folder/file to new path ([#92](https://github.com/sigoden/dufs/issues/92))
- Check permission on move/copy destination ([#93](https://github.com/sigoden/dufs/issues/93))
- Add completions ([#97](https://github.com/sigoden/dufs/issues/97))
- Limit the number of concurrent uploads ([#98](https://github.com/sigoden/dufs/issues/98))

## [0.24.0] - 2022-07-02

### Bug Fixes

- Unexpected stack overflow when searching a lot ([#87](https://github.com/sigoden/dufs/issues/87))

### Features

- Allow search with --render-try-index ([#88](https://github.com/sigoden/dufs/issues/88))

## [0.23.1] - 2022-06-30

### Bug Fixes

- Safari layout and compatibility ([#83](https://github.com/sigoden/dufs/issues/83))
- Permissions of unzipped files ([#84](https://github.com/sigoden/dufs/issues/84))

## [0.23.0] - 2022-06-29

### Features

- Use feature to conditional support tls ([#77](https://github.com/sigoden/dufs/issues/77))

### Ci

- Support more platforms ([#76](https://github.com/sigoden/dufs/issues/76))

## [0.22.0] - 2022-06-26

### Features

- Support hiding folders with --hidden ([#73](https://github.com/sigoden/dufs/issues/73))

## [0.21.0] - 2022-06-23

### Bug Fixes

- Escape name contains html escape code ([#65](https://github.com/sigoden/dufs/issues/65))

### Features

- Use custom logger with timestamp in rfc3339 ([#67](https://github.com/sigoden/dufs/issues/67))

### Refactor

- Split css/js from index.html ([#68](https://github.com/sigoden/dufs/issues/68))

## [0.20.0] - 2022-06-20

### Bug Fixes

- DecodeURI searching string ([#61](https://github.com/sigoden/dufs/issues/61))

### Features

- Added basic auth ([#60](https://github.com/sigoden/dufs/issues/60))
- Add option --allow-search ([#62](https://github.com/sigoden/dufs/issues/62))

## [0.19.0] - 2022-06-19

### Features

- [**breaking**] Path level access control ([#52](https://github.com/sigoden/dufs/issues/52))
- Serve single file ([#54](https://github.com/sigoden/dufs/issues/54))
- Ui hidden root dirname ([#58](https://github.com/sigoden/dufs/issues/58))
- Reactive webpage ([#51](https://github.com/sigoden/dufs/issues/51))
- [**breaking**] Rename to dufs ([#59](https://github.com/sigoden/dufs/issues/59))

### Refactor

- [**breaking**] Rename --cors to --enable-cors ([#57](https://github.com/sigoden/dufs/issues/57))

## [0.18.0] - 2022-06-18

### Features

- Add option --render-try-index ([#47](https://github.com/sigoden/dufs/issues/47))
- Add slash to end of dir href

## [0.17.1] - 2022-06-16

### Bug Fixes

- Range request ([#44](https://github.com/sigoden/dufs/issues/44))

## [0.17.0] - 2022-06-15

### Bug Fixes

- Webdav propfind dir with slash ([#42](https://github.com/sigoden/dufs/issues/42))

### Features

- Listen both ipv4 and ipv6 by default ([#40](https://github.com/sigoden/dufs/issues/40))

### Refactor

- Trivial changes ([#41](https://github.com/sigoden/dufs/issues/41))

## [0.16.0] - 2022-06-12

### Features

- Implement head method ([#33](https://github.com/sigoden/dufs/issues/33))
- Display upload speed and time left ([#34](https://github.com/sigoden/dufs/issues/34))
- Support tls-key in pkcs#8 format ([#35](https://github.com/sigoden/dufs/issues/35))
- Options method return status 200

### Testing

- Add integration tests ([#36](https://github.com/sigoden/dufs/issues/36))

## [0.15.1] - 2022-06-11

### Bug Fixes

- Cannot upload ([#32](https://github.com/sigoden/dufs/issues/32))

## [0.15.0] - 2022-06-10

### Bug Fixes

- Encode webdav href as uri ([#28](https://github.com/sigoden/dufs/issues/28))
- Query dir param

### Features

- Add basic dark theme ([#29](https://github.com/sigoden/dufs/issues/29))
- Add empty state placeholder to page([#30](https://github.com/sigoden/dufs/issues/30))

## [0.14.0] - 2022-06-07

### Bug Fixes

- Send index page with content-type ([#26](https://github.com/sigoden/dufs/issues/26))

### Features

- Support ipv6 ([#25](https://github.com/sigoden/dufs/issues/25))
- Add favicon ([#27](https://github.com/sigoden/dufs/issues/27))

## [0.13.2] - 2022-06-06

### Bug Fixes

- Filename xml escaping
- Escape path-prefix/url-prefix different

## [0.13.1] - 2022-06-05

### Bug Fixes

- Escape filename ([#21](https://github.com/sigoden/dufs/issues/21))

### Refactor

- Use logger ([#22](https://github.com/sigoden/dufs/issues/22))

## [0.13.0] - 2022-06-05

### Bug Fixes

- Ctrl+c not exit sometimes

### Features

- Implement more webdav methods ([#13](https://github.com/sigoden/dufs/issues/13))
- Use digest auth ([#14](https://github.com/sigoden/dufs/issues/14))
- Add webdav proppatch handler ([#18](https://github.com/sigoden/dufs/issues/18))

## [0.12.1] - 2022-06-04

### Features

- Support webdav ([#10](https://github.com/sigoden/dufs/issues/10))
- Remove unzip uploaded feature ([#11](https://github.com/sigoden/dufs/issues/11))

## [0.11.0] - 2022-06-03

### Features

- Support gracefully shutdown server
- Listen 0.0.0.0 by default

## [0.10.1] - 2022-06-02

### Bug Fixes

- Panic when bind already used port

## [0.10.0] - 2022-06-02

### Bug Fixes

- Remove unzip file even failed to unzip
- Rename --no-auth-read to --no-auth-access
- Broken ui

### Documentation

- Refactor readme

### Features

- Change auth logic/options
- Improve ui

### Refactor

- Small improvement

## [0.9.0] - 2022-06-02

### Documentation

- Improve readme

### Features

- Support path prefix
- List all ifaces when listening 0.0.0.0
- Support tls

## [0.8.0] - 2022-06-01

### Bug Fixes

- Some typos
- Caught 500 if no permission to access dir

### Features

- Cli add allow-symlink option
- Add some headers to res
- Support render-index/render-spa

## [0.7.0] - 2022-05-31

### Bug Fixes

- Downloaded zip file has no.zip ext in firefox
- Unzip override existed file in uploadonly mode
- Miss file 500
- Not found dir when allow_upload is false

### Features

- Drag and drop uploads, upload folder

## [0.6.0] - 2022-05-31

### Features

- Delete confirm
- Distinct upload and delete operation
- Support range requests

### Refactor

- Improve code quality

## [0.5.0] - 2022-05-30

### Features

- Add mime and cache headers to response
- Add no-auth-read options
- Unzip zip file when unload

## [0.4.0] - 2022-05-29

### Features

- Replace --static option to --no-edit
- Add cors

## [0.3.0] - 2022-05-29

### Documentation

- Update readme demo png

### Features

- Automatically create dir while uploading
- Support searching

### Refactor

- Handler zip

### Styling

- Optimize css

## [0.2.1] - 2022-05-28

### Bug Fixes

- Cannot upload in root
- Optimize download zip

### Documentation

- Improve readme

### Features

- Aware RUST_LOG

## [0.2.0] - 2022-05-28

### Documentation

- Update demo png
- Improve readme

### Features

- Add logger
- Download folder as zip file

## [0.1.0] - 2022-05-26

### Bug Fixes

- Caught server error when symlink broken

### Documentation

- Improve readme
- Update readme

### Features

- Add basic auth and readonly mode
- Support delete operation
- Remove parent path

### Styling

- Cargo fmt
- Update index page

### Build

- Remove dev deps

### Ci

- Init ci

<!-- generated by git-cliff -->
