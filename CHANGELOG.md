# Change Log

## 0.3.0: 2017-09-21
This release starts from adding `--height` featuren, ends up a big change in
the code base.
- feature: `--bind` accept character keys. Only Ctrl/Alt/F keys were accepted.
- feature: support multiple `--bind` options. (replace getopts with clap.rs)
- feature: `--tac` to reverse the order of input lines.
- feature: `--preview` to show preview of current selected line.
- feature: `--height` to use only part instead of full of the screen.
- test: use tmux for integration test
- replace [ncurses-rs](https://github.com/jeaye/ncurses-rs) with [termion](https://github.com/ticki/termion), now skim is fully rust, no C bindings.
