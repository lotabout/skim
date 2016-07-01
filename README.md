FZF in Rust. [fzf](https://github.com/junegunn/fzf) is a command-line fuzzy finder written in Go while [fzf-rs](https://github.com/lotabout/fzf-rs) is a re-implementation in Rust!

TODO: add gif

# Usage

Current requires nightly rust to build. clone the repo and run:

```
cargo build --release
```

and put the resulting `target/release/fzf-rs` executable on your PATH.

Now try out the following commands:

```
# directly input fzf-rs
fzf-rs

# or pipe some input to it: (press TAB key select multiple items with -m enabled)
vim $(find . -name "*.rs" | fzf-rs -m)
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

Basically keys work in fzf will work in `fzf-rs`.

# About fzf-rs

One target of `fzf-rs` is to be compatible with `fzf`, so that we can re-use
all the plugins(such as fzf.vim) that comes with `fzf`. Of course now it is
far from finished.

## Difference to fzf

This project is written from scratch. Some decisions of impelmentation are
different from fzf. For example:

1. The fuzzy search algorithm is different.
2. UI of showing matched items. `fzf` will show only the range matched while
   `fzf-rs` will show each character matched.
3. The implementation details are quite different if you care.

## How to contribute

Feel free to [create new
issue](https://github.com/lotabout/fzf-rs/issues/new) if you meet any bugs
or have any ideas.
