# Key bindings
# ------------
# copied and modified from https://github.com/junegunn/fzf/blob/master/shell/key-bindings.bash
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
  line=$(
    builtin fc -lnr -2147483648 |
      perl -p -l0 -e 'BEGIN { getc; $/ = "\n\t" } s/^[ *]//; $_ = '"$1"' - $. . "\t$_"' |
      SKIM_DEFAULT_OPTIONS="--height ${SKIM_TMUX_HEIGHT:-40%} $SKIM_DEFAULT_OPTIONS --tiebreak=score,index $SKIM_CTRL_R_OPTS -m --read0" $(__skimcmd)
  )
  echo "${line#*$'\t'}"
)

# Required to refresh the prompt after skim
bind -m emacs-standard '"\er": redraw-current-line'

# CTRL-T - Paste the selected file path into the command line
if [ $BASH_VERSINFO -gt 3 ]; then
  bind -m emacs-standard -x '"\C-t": "skim-file-widget"'
elif __skim_use_tmux__; then
  bind -m emacs-standard '"\C-t": " \C-b\C-k \C-u`__skim_select_tmux__`\e\C-e\C-a\C-y\C-h\C-e\e \C-y\ey\C-x\C-x\C-f"'
else
  bind -m emacs-standard '"\C-t": " \C-b\C-k \C-u`__skim_select__`\e\C-e\er\C-a\C-y\C-h\C-e\e \C-y\ey\C-x\C-x\C-f"'
fi

# CTRL-R - Paste the selected command from history into the command line
bind -m emacs-standard '"\C-r": "\C-e \C-u\C-y\ey\C-u__skim_history__ $HISTCMD\e\C-e`"\C-a"`\C-e\e\C-e\er"'

# ALT-C - cd into the selected directory
bind -m emacs-standard '"\ec": " \C-b\C-k \C-u`__skim_cd__`\e\C-e\er\C-m\C-y\C-h\e \C-y\ey\C-x\C-x\C-d"'

bind -m vi-command '"\C-z": emacs-editing-mode'
bind -m vi-insert '"\C-z": emacs-editing-mode'
bind -m emacs-standard '"\C-z": vi-editing-mode'

# CTRL-T - Paste the selected file path into the command line
bind -m vi-command '"\C-t": "\C-z\C-t\C-z"'
bind -m vi-insert '"\C-t": "\C-z\C-t\C-z"'

# CTRL-R - Paste the selected command from history into the command line
bind -m vi-command '"\C-r": "\C-z\C-r\C-z"'
bind -m vi-insert '"\C-r": "\C-z\C-r\C-z"'

# ALT-C - cd into the selected directory
bind -m vi-command '"\ec": "\C-z\ec\C-z"'
bind -m vi-insert '"\ec": "\C-z\ec\C-z"'

fi
