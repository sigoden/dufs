# Duf

[![CI](https://github.com/sigoden/duf/actions/workflows/ci.yaml/badge.svg)](https://github.com/sigoden/duf/actions/workflows/ci.yaml)
[![Crates](https://img.shields.io/crates/v/duf.svg)](https://crates.io/crates/duf)

Duf is a simple file server.

![demo](https://user-images.githubusercontent.com/4012553/170485306-aec36bf7-bcf7-46cb-ae70-6358ebdce0d6.png)

## Features

- Serve static files
- Upload/Delete files
- Support basic auth

## Install

### With cargo

```
cargo install duf
```

### Binaries on macOS, Linux, Windows

Download from [Github Releases](https://github.com/sigoden/duf/releases), unzip and add duf to your $PATH.

## Usage

You can run this command to start serving your current working directory on 127.0.0.1:5000 by default.

```
duf
```

...or specify which folder you want to serve:

```
duf folder_name
```

Finally, run this command to see a list of all available option


You can upload file to server with curl.

```
curl --upload-file some-file http://127.0.0.1:5000/some-file
```
... or delete file/folder with curl

```
curl -X DELETE http://127.0.0.1:5000/some-file
```

## License

Copyright (c) 2022 duf-developers.

duf is made available under the terms of either the MIT License or the Apache License 2.0, at your option.

See the LICENSE-APACHE and LICENSE-MIT files for license details.