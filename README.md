# rust-tfm

`rust-tfm` is a terminal file manager written in rust heavily inspired by [lf](https://github.com/gokcehan/lf) and [ranger](https://github.com/ranger/ranger), and also inspired by [suckless software](https://suckless.org/).

[![Preview](https://github.com/jiaoshijie/rust-tfm/assets/43605101/246b0a58-e604-4b13-9449-f991039b6f55)](https://github.com/jiaoshijie/rust-tfm/assets/43605101/6a5b1e3a-52e6-4ae9-b85d-eb8c0dd16308)

## TODO

- [ ] Change nav.selections type from HashMap to BTreeMap

## Features

- Asynchronous IO operations to avoid UI blocking
- Customizable keybindings(vi-style by default)
- Extendable and configurable with shell commands
- Using `src/config.rs` to config tfm, like [dwm](https://dwm.suckless.org/), [st](https://st.suckless.org/), and [dmenu](https://tools.suckless.org/dmenu/).

## Non-Features

- Cross-platform, works on linux(tested on Arch and Fedora38) and maybe works on macOS(not tested).
- Tabs or windows (better handled by window manager or terminal multiplexer)

## Installation

Currently, you can only install it by cloning this repository and building it on your own.

```bash
git clone https://github.com/jiaoshjie/rust-tfm.git

cd rust-tfm

cargo build --release
# or
cargo install --path .
```

The binary file is `tfm` not rust-tfm.

## Config

Like suckless software, modify the `src/config.rs` and then rebuild it.

## Usage

See `src/config.rs` or maybe all source files.

## Dependencies(Optional)

You can modify `script/preview` and `script/open` files to change the behavior of preview and open as you like.

### Preview

Below are some defalut `script/preview` dependencies.

1. `lynx` html files
2. `bat` color highlight for text files
3. `chafa` image files
4. `mediainfo` audio files
5. `ffmpegthumbnailer` video to image
6. `atool` zip files

### Open

1. `mpv` video and audio files
2. `sxiv` image files and a `rotdir` script which can be found [here](https://github.com/jiaoshijie/dots/blob/main/scripts/bin/rotdir).
3. `display` svg files
4. Set `$EDITOR` and `$BROWSER` env variables for text files and pdf files

## License

**MIT**
