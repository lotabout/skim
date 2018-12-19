# Key bindings
# ------------
# copied and modified from https://github.com/junegunn/fzf/blob/master/shell/key-bindings.bash
#
__skim_select__() {
  local cmd="${SKIM_CTRL_T_COMMAND:-"command find -L . -mindepth 1 \\( -path '*/\\.*' -o -fstype 'sysfs' -o -fstype 'devfs' -o -fstype 'devtmpfs' -o -fstype 'proc' \\) -prune \
    -o -type f -print \
    -o -type d -print \
    -o -type l -print 2> /dev/null | cut -b3-"}"
  eval "$cmd" | SKIM_DEFAULT_OPTIONS="--height ${SKIM_TMUX_HEIGHT:-40%} --reverse $SKIM_DEFAULT_OPTIONS $SKIM_CTRL_T_OPTS" sk -m "$@" | while read -r item; do
    printf '%q ' "$item"
  done
  echo
}

if [[ $- =~ i ]]; then

__skim_use_tmux__() {
  [ -n "$TMUX_PANE" ] && [ "${SKIM_TMUX:-0}" != 0 ] && [ ${LINES:-40} -gt 15 ]
}

__skimcmd() {
  __skim_use_tmux__ &&
    echo "sk-tmux -d${SKIM_TMUX_HEIGHT:-40%}" || echo "sk"
}

__skim_select_tmux__() {
  local height
  height=${SKIM_TMUX_HEIGHT:-40%}
  if [[ $height =~ %$ ]]; then
    height="-p ${height%\%}"
  else
    height="-l $height"
  fi

  tmux split-window $height "cd $(printf %q "$PWD"); SKIM_DEFAULT_OPTIONS=$(printf %q "$SKIM_DEFAULT_OPTIONS") PATH=$(printf %q "$PATH") SKIM_CTRL_T_COMMAND=$(printf %q "$SKIM_CTRL_T_COMMAND") SKIM_CTRL_T_OPTS=$(printf %q "$SKIM_CTRL_T_OPTS") bash -c 'source \"${BASH_SOURCE[0]}\"; RESULT=\"\$(__skim_select__ --no-height)\"; tmux setb -b skim \"\$RESULT\" \\; pasteb -b skim -t $TMUX_PANE \\; deleteb -b skim || tmux send-keys -t $TMUX_PANE \"\$RESULT\"'"
}

skim-file-widget() {
  if __skim_use_tmux__; then
    __skim_select_tmux__
  else
    local selected="$(__skim_select__)"
    READLINE_LINE="${READLINE_LINE:0:$READLINE_POINT}$selected${READLINE_LINE:$READLINE_POINT}"
    READLINE_POINT=$(( READLINE_POINT + ${#selected} ))
  fi
}

__skim_cd__() {
  local cmd dir
  cmd="${SKIM_ALT_C_COMMAND:-"command find -L . -mindepth 1 \\( -path '*/\\.*' -o -fstype 'sysfs' -o -fstype 'devfs' -o -fstype 'devtmpfs' -o -fstype 'proc' \\) -prune \
    -o -type d -print 2> /dev/null | cut -b3-"}"
  dir=$(eval "$cmd" | SKIM_DEFAULT_OPTIONS="--height ${SKIM_TMUX_HEIGHT:-40%} --reverse $SKIM_DEFAULT_OPTIONS $SKIM_ALT_C_OPTS" $(__skimcmd) -m) && printf 'cd %q' "$dir"
}

__skim_history__() (
  local line
  shopt -u nocaseglob nocasematch
  line=$(
    HISTTIMEFORMAT= history |
    SKIM_DEFAULT_OPTIONS="--height ${SKIM_TMUX_HEIGHT:-40%} $SKIM_DEFAULT_OPTIONS --tac --sync -n2..,.. --tiebreak=index $SKIM_CTRL_R_OPTS -m" $(__skimcmd) |
    command grep '^ *[0-9]') &&
    if [[ $- =~ H ]]; then
      sed 's/^ *\([0-9]*\)\** .*/!\1/' <<< "$line"
    else
      sed 's/^ *\([0-9]*\)\** *//' <<< "$line"
    fi
)

if [[ ! -o vi ]]; then
  # Required to refresh the prompt after skim
  bind '"\er": redraw-current-line'
  bind '"\e^": history-expand-line'

  # CTRL-T - Paste the selected file path into the command line
  if [ $BASH_VERSINFO -gt 3 ]; then
    bind -x '"\C-t": "skim-file-widget"'
  elif __skim_use_tmux__; then
    bind '"\C-t": " \C-u \C-a\C-k`__skim_select_tmux__`\e\C-e\C-y\C-a\C-d\C-y\ey\C-h"'
  else
    bind '"\C-t": " \C-u \C-a\C-k`__skim_select__`\e\C-e\C-y\C-a\C-y\ey\C-h\C-e\er \C-h"'
  fi

  # CTRL-R - Paste the selected command from history into the command line
  bind '"\C-r": " \C-e\C-u\C-y\ey\C-u`__skim_history__`\e\C-e\er\e^"'

  # ALT-C - cd into the selected directory
  bind '"\ec": " \C-e\C-u`__skim_cd__`\e\C-e\er\C-m"'
else
  # We'd usually use "\e" to enter vi-movement-mode so we can do our magic,
  # but this incurs a very noticeable delay of a half second or so,
  # because many other commands start with "\e".
  # Instead, we bind an unused key, "\C-x\C-a",
  # to also enter vi-movement-mode,
  # and then use that thereafter.
  # (We imagine that "\C-x\C-a" is relatively unlikely to be in use.)
  bind '"\C-x\C-a": vi-movement-mode'

  bind '"\C-x\C-e": shell-expand-line'
  bind '"\C-x\C-r": redraw-current-line'
  bind '"\C-x^": history-expand-line'

  # CTRL-T - Paste the selected file path into the command line
  # - FIXME: Selected items are attached to the end regardless of cursor position
  if [ $BASH_VERSINFO -gt 3 ]; then
    bind -x '"\C-t": "skim-file-widget"'
  elif __skim_use_tmux__; then
    bind '"\C-t": "\C-x\C-a$a \C-x\C-addi`__skim_select_tmux__`\C-x\C-e\C-x\C-a0P$xa"'
  else
    bind '"\C-t": "\C-x\C-a$a \C-x\C-addi`__skim_select__`\C-x\C-e\C-x\C-a0Px$a \C-x\C-r\C-x\C-axa "'
  fi
  bind -m vi-command '"\C-t": "i\C-t"'

  # CTRL-R - Paste the selected command from history into the command line
  bind '"\C-r": "\C-x\C-addi`__skim_history__`\C-x\C-e\C-x\C-r\C-x^\C-x\C-a$a"'
  bind -m vi-command '"\C-r": "i\C-r"'

  # ALT-C - cd into the selected directory
  bind '"\ec": "\C-x\C-addi`__skim_cd__`\C-x\C-e\C-x\C-r\C-m"'
  bind -m vi-command '"\ec": "ddi`__skim_cd__`\C-x\C-e\C-x\C-r\C-m"'
fi

fi
