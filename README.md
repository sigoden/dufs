# Duf

[![CI](https://github.com/sigoden/duf/actions/workflows/ci.yaml/badge.svg)](https://github.com/sigoden/duf/actions/workflows/ci.yaml)
[![Crates](https://img.shields.io/crates/v/duf.svg)](https://crates.io/crates/duf)

Duf is a fully functional file server.

![demo](https://user-images.githubusercontent.com/4012553/170853369-dfcdc4c7-4864-4401-bf49-6e41ffe05df3.png)

## Features

- Serve static files
- Download folder as zip file
- Search files
- Upload files and folders
- Delete files
- Basic authentication
- Upload zip file then unzip
- Easy to use with curl

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

...or specify which folder you want to serve.

```
duf folder_name
```

Allow all operations such as upload, delete

```
duf --allow-all
```

Only allow upload operations

```
duf --allow-upload
```

Use http authentication

```
duf --auth user:pass
```

### Api

Download a file
```
curl http://127.0.0.1:5000/some-file

curl -o some-file2 http://127.0.0.1:5000/some-file
```

Download a folder as zip file

```
curl -o some-folder.zip http://127.0.0.1:5000/some-folder?zip
```

Upload a file

```
curl --upload-file some-file http://127.0.0.1:5000/some-file
```

Unzip zip file when unload

```
curl --upload-file some-folder.zip http://127.0.0.1:5000/some-folder.zip?unzip
```

Delete a file/folder

```
curl -X DELETE http://127.0.0.1:5000/some-file
```

## License

Copyright (c) 2022 duf-developers.

duf is made available under the terms of either the MIT License or the Apache License 2.0, at your option.

See the LICENSE-APACHE and LICENSE-MIT files for license details.