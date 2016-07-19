Please do NOT clone the project for now, I'm testing the CI build.

Fuzzy Finder in Rust.

TODO: add a new gif demo with new name.

# Usage

Current requires nightly rust to build. clone the repo and run:

```
cargo build --release
```

and put the resulting `target/release/skim` executable on your PATH.

Now try out the following commands:

```
# directly invoke skim
skim

# or pipe some input to it: (press TAB key select multiple items with -m enabled)
vim $(find . -name "*.rs" | skim -m)
```
The above command will allow you to select files with ".rs" extension and open
the ones you selected in vim.

## Key bindings

Some common used keybindings.

| key | Action |
|---:|---|
| Enter | Accept (select current one and quit)  |
| ESC/Ctrl-G/Ctrl-Q | Abort|
| Ctrl-P/Up | Move cursor up|
| Ctrl-N/Down | Move cursor Down|
| TAB | Toggle selection and move down (with `-m`)|
| Shift-TAB | Toggle selection and move up (with `-m`)|

Basically keys work in fzf will work in `skim`.

# About skim

[fzf](https://github.com/junegunn/fzf) is a command-line fuzzy finder written
in Go and [skim](https://github.com/lotabout/skim) trys to implement a new one
in Rust!

One target of `skim` is to be compatible with `fzf`, so that we can re-use
all the plugins(such as fzf.vim) that comes with `fzf`. Of course now it is
far from finished.

## Difference to fzf

This project is written from scratch. Some decisions of impelmentation are
different from fzf. For example:

1. The fuzzy search algorithm is different.
2. UI of showing matched items. `fzf` will show only the range matched while
   `skim` will show each character matched.
3. The implementation details are quite different if you care.

## How to contribute

Feel free to [create new
issue](https://github.com/lotabout/skim/issues/new) if you meet any bugs
or have any ideas.

# Manual

## exit code

| Exit Code | Meaning |
|---|---|
| 0 | Exit normally |
| 1 | No Match found |
| 130 | Abort by Ctrl-C/Ctrl-G/ESC/etc... |
