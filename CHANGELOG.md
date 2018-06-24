# Change Log

## 0.5.1: 2018-06-24

Use [cross](https://github.com/japaric/cross) to build targets.

## 0.5.0: 2018-06-12

Change the field syntax to be fzf compatible.
- Previously it was git style
    - fields starts with `0`
    - `1..3` results in `2, 3` (which is `0, 1, 2, 3` minus `0, 1`)
- Now it is `cut` style
    - fields starts with `1`
    - `1..3` results in `1, 2, 3`

## 0.4.0: 2018-06-03

Refactor skim into a library. With minor bug fixes:
- support multiple arguments, to be a drop-in replacement of fzf.
- support negative range field. (e.g. `-1` to specify the last field)
- respond to terminal resize event on Mac.

## 0.3.2: 2018-01-18
Some minor enhancements that might comes handy.
- Reserve all fzf options, so that skim can be a drop-in replacement of fzf.
- Fix: the number of columns a unicode character occupies
- Accept multiple values for most options. So that you can safely put them
    in `$SKIM_DEFAULT_OPTIONS` and override it in command line.

Thanks to @magnetophon for the bug report and feature requests.

## 0.3.1: 2017-12-04
Support more options, and reserve several others. The purpose is to reuse
`fzf.vim` as much as possible.
- `--print0`: use NUL(\0) as field separator for output.
- `--read0`: read input delimited by NUL(\0) characters
- `--tabstop`: allow customizing tabstop (default to 8).
- `--no-hscroll`: disable hscroll on match.
- reserve several other options, skim will do nothing on them instead of throwing errors.

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
