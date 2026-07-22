# Rust dependency and license review for 0.2.0

Reviewed on 2026-07-21 against `Cargo.lock` with the release target
`x86_64-unknown-linux-gnu`. The scope is the normal dependency graph reported by
`cargo tree --locked --edges normal --target x86_64-unknown-linux-gnu` for the
`codex-voice` binary. Build-only and non-Linux target dependencies in the lock
file are not shipped in that binary, but are inventoried below.

All reviewed terms permit binary redistribution. Where a crate offers
`MIT OR Apache-2.0`, or the historical equivalent `MIT/Apache-2.0`, this package
selects MIT and reproduces the MIT terms in `debian/copyright`. Other required
notices are reproduced there as well.

| Crate | Version | Declared license | Distribution choice |
| --- | --- | --- | --- |
| bitflags | 2.13.0 | MIT OR Apache-2.0 | MIT |
| fallible-iterator | 0.3.0 | MIT/Apache-2.0 | MIT |
| fallible-streaming-iterator | 0.1.9 | MIT/Apache-2.0 | MIT |
| foldhash | 0.2.0 | Zlib | Zlib |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 | MIT |
| hashlink | 0.12.1 | MIT OR Apache-2.0 | MIT |
| itoa | 1.0.18 | MIT OR Apache-2.0 | MIT |
| libc | 0.2.186 | MIT OR Apache-2.0 | MIT |
| libsqlite3-sys | 0.38.1 | MIT | MIT |
| memchr | 2.8.3 | Unlicense OR MIT | MIT |
| proc-macro2 | 1.0.106 | MIT OR Apache-2.0 | MIT |
| quote | 1.0.46 | MIT OR Apache-2.0 | MIT |
| rusqlite | 0.40.1 | MIT | MIT |
| serde | 1.0.228 | MIT OR Apache-2.0 | MIT |
| serde_core | 1.0.228 | MIT OR Apache-2.0 | MIT |
| serde_derive | 1.0.228 | MIT OR Apache-2.0 | MIT |
| serde_json | 1.0.150 | MIT OR Apache-2.0 | MIT |
| smallvec | 1.15.2 | MIT OR Apache-2.0 | MIT |
| syn | 2.0.118 | MIT OR Apache-2.0 | MIT |
| unicode-ident | 1.0.24 | (MIT OR Apache-2.0) AND Unicode-3.0 | MIT AND Unicode-3.0 |
| zmij | 1.0.21 | MIT | MIT |

Build-only crates are `cc 1.2.67`, `find-msvc-tools 0.1.9`, `pkg-config
0.3.33`, `shlex 2.0.1`, and `vcpkg 0.2.15`; each permits MIT selection. The
remaining target-specific locked crates are `bumpalo 3.20.3`, `cfg-if 1.0.4`,
`hashbrown 0.16.1`, `js-sys 0.3.103`, `once_cell 1.21.4`, `rsqlite-vfs 0.1.1`,
`rustversion 1.0.23`, `sqlite-wasm-rs 0.5.5`, `thiserror 2.0.18`,
`thiserror-impl 2.0.18`, and the `wasm-bindgen 0.2.126` macro, macro-support,
shared, and runtime crates. These are MIT or permit MIT selection and are not
compiled for the release target.

## Bundled SQLite

`rusqlite 0.40.1` enables its `bundled` feature. Consequently,
`libsqlite3-sys 0.38.1` compiles and statically links its SQLite 3.53.2
amalgamation rather than dynamically using Debian's `libsqlite3` package. The
amalgamation states that its authors disclaim copyright and supplies the SQLite
blessing in place of a license. The public-domain statement and blessing are in
`debian/copyright` and the installed package copyright file.

## Separately bundled codex-asr executable

`codex-asr 0.1.2` is not part of the Codex Voice Cargo graph. It is the upstream
v0.1.2 release at commit `7157050052c769b3fec464e5c5a7b7422b95b10d`, pinned by
archive and binary SHA-256. Upstream declares MIT and its `LICENSE-MIT` notice
is Copyright (c) 2026 codex-asr contributors.

Its default-feature Linux graph was separately reviewed with `cargo tree
--locked --edges normal --target x86_64-unknown-linux-gnu` against the upstream
v0.1.2 lock file. The distribution choices are:

- `MIT`: anstream 1.0.0, anstyle 1.0.14, anstyle-parse 1.0.0, anstyle-query
  1.1.5, atomic-waker 1.1.2, axum 0.8.9, axum-core 0.5.6, base64 0.22.1,
  bitflags 2.11.1, bytes 1.11.1, cfg-if 1.0.4, clap 4.6.1, clap_builder 4.6.0,
  clap_derive 4.6.1, clap_lex 1.1.0, colorchoice 1.0.5, displaydoc 0.2.5,
  errno 0.3.14, fastrand 2.4.1, form_urlencoded 1.2.2, futures-channel 0.3.32,
  futures-core 0.3.32, futures-io 0.3.32, futures-sink 0.3.32, futures-task
  0.3.32, futures-util 0.3.32, getrandom 0.2.17 and 0.3.4, heck 0.5.0, http
  1.4.0, http-body 1.0.1, http-body-util 0.1.3, httparse 1.10.1, httpdate
  1.0.3, hyper 1.9.0, hyper-rustls 0.27.9, hyper-util 0.1.20, idna 1.1.0,
  idna_adapter 1.2.2, ipnet 2.12.0, iri-string 0.7.12,
  is_terminal_polyfill 1.70.2, itoa 1.0.18, libc 0.2.186, linux-raw-sys
  0.12.1, log 0.4.29, memchr 2.8.0, mime 0.3.17, mime_guess 2.0.5, mio
  1.2.0, multer 3.1.0, once_cell 1.21.4, percent-encoding 2.3.2,
  pin-project-lite 0.2.17, proc-macro2 1.0.106, quote 1.0.45, reqwest
  0.12.28, rustix 1.1.4, rustls 0.23.40, rustls-pki-types 1.14.1, serde
  1.0.228, serde_core 1.0.228, serde_derive 1.0.228, serde_json 1.0.149,
  serde_path_to_error 0.1.20, serde_urlencoded 0.7.1, signal-hook-registry
  1.4.8, slab 0.4.12, smallvec 1.15.1, socket2 0.6.3, spin 0.9.8,
  stable_deref_trait 1.2.1, strsim 0.11.1, syn 2.0.117, synstructure 0.13.2,
  tempfile 3.27.0, thiserror 2.0.18, thiserror-impl 2.0.18, tokio 1.52.2,
  tokio-macros 2.7.0, tokio-rustls 0.26.4, tower 0.5.3, tower-http 0.6.8,
  tower-layer 0.3.3, tower-service 0.3.3, tracing 0.1.44, tracing-core
  0.1.36, try-lock 0.2.5, unicase 2.9.0, url 2.5.8, utf8_iter 1.0.4,
  utf8parse 0.2.2, want 0.3.1, zeroize 1.8.2, and zmij 1.0.21.
- `Unicode-3.0`: icu_collections 2.2.0, icu_locale_core 2.2.0,
  icu_normalizer 2.2.0, icu_normalizer_data 2.2.0, icu_properties 2.2.0,
  icu_properties_data 2.2.0, icu_provider 2.2.0, litemap 0.8.2,
  potential_utf 0.1.5, tinystr 0.8.3, writeable 0.6.3, yoke 0.8.2,
  yoke-derive 0.8.2, zerofrom 0.1.7, zerofrom-derive 0.1.7, zerotrie 0.2.4,
  zerovec 0.11.6, and zerovec-derive 0.11.3.
- `MIT AND Unicode-3.0`: unicode-ident 1.0.24.
- `MIT AND BSD-3-Clause`: encoding_rs 0.8.35 and matchit 0.8.4.
- `Apache-2.0 AND ISC`: ring 0.17.14.
- `Apache-2.0`: ryu 1.0.23 and sync_wrapper 1.0.2.
- `ISC`: rustls-webpki 0.103.13 and untrusted 0.9.0.
- `BSD-3-Clause`: subtle 2.6.1.
- `CDLA-Permissive-2.0`: webpki-roots 1.0.7.

The codex-asr notice, selected license texts, and required third-party terms are
in `debian/copyright` and the installed package copyright file. This review
assumes the pinned release executable was built by upstream from the tagged,
locked default-feature graph; the matching version and upstream-provided SHA-256
pin support that assumption, but the stripped binary cannot independently prove
its complete source composition.
