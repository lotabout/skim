#     ____      ____
#    / __/___  / __/
#   / /_/_  / / /_
#  / __/ / /_/ __/
# /_/   /___/_/ key-bindings.zsh
#
# - $SKIM_TMUX_OPTS
# - $SKIM_CTRL_T_COMMAND
# - $SKIM_CTRL_T_OPTS
# - $SKIM_CTRL_R_OPTS
# - $SKIM_ALT_C_COMMAND
# - $SKIM_ALT_C_OPTS

# Key bindings
# ------------

# The code at the top and the bottom of this file is the same as in completion.zsh.
# Refer to that file for explanation.
if 'zmodload' 'zsh/parameter' 2>'/dev/null' && (( ${+options} )); then
  __skim_key_bindings_options="options=(${(j: :)${(kv)options[@]}})"
else
  () {
    __skim_key_bindings_options="setopt"
    'local' '__skim_opt'
    for __skim_opt in "${(@)${(@f)$(set -o)}%% *}"; do
      if [[ -o "$__skim_opt" ]]; then
        __skim_key_bindings_options+=" -o $__skim_opt"
      else
        __skim_key_bindings_options+=" +o $__skim_opt"
      fi
    done
  }
fi

'emulate' 'zsh' '-o' 'no_aliases'

{

[[ -o interactive ]] || return 0

# CTRL-T - Paste the selected file path(s) into the command line
__fsel() {
  local cmd="${SKIM_CTRL_T_COMMAND:-"command find -L . -mindepth 1 \\( -path '*/\\.*' -o -fstype 'sysfs' -o -fstype 'devfs' -o -fstype 'devtmpfs' -o -fstype 'proc' \\) -prune \
    -o -type f -print \
    -o -type d -print \
    -o -type l -print 2> /dev/null | cut -b3-"}"
  setopt localoptions pipefail no_aliases 2> /dev/null
  REPORTTIME_=$REPORTTIME
  unset REPORTTIME
  eval "$cmd" | SKIM_DEFAULT_OPTIONS="--height ${SKIM_TMUX_HEIGHT:-40%} --reverse $SKIM_DEFAULT_OPTIONS $SKIM_CTRL_T_OPTS" $(__skimcmd) -m "$@" | while read item; do
    echo -n "${(q)item} "
  done
  local ret=$?
  echo
  if ! [ -z $REPORTTIME_ ]; then
      REPORTTIME=$REPORTTIME_
  fi
  unset REPORTTIME_
  return $ret
}

__skimcmd() {
  [ -n "$TMUX_PANE" ] && { [ "${SKIM_TMUX:-0}" != 0 ] || [ -n "$SKIM_TMUX_OPTS" ]; } &&
    echo "sk-tmux ${SKIM_TMUX_OPTS:--d${SKIM_TMUX_HEIGHT:-40%}} -- " || echo "sk"
}

skim-file-widget() {
  LBUFFER="${LBUFFER}$(__fsel)"
  local ret=$?
  zle reset-prompt
  return $ret
}
zle     -N   skim-file-widget
bindkey '^T' skim-file-widget

# Ensure precmds are run after cd
skim-redraw-prompt() {
  local precmd
  for precmd in $precmd_functions; do
    $precmd
  done
  zle reset-prompt
}
zle -N skim-redraw-prompt

# ALT-C - cd into the selected directory
skim-cd-widget() {
  local cmd="${SKIM_ALT_C_COMMAND:-"command find -L . -mindepth 1 \\( -path '*/\\.*' -o -fstype 'sysfs' -o -fstype 'devfs' -o -fstype 'devtmpfs' -o -fstype 'proc' \\) -prune \
    -o -type d -print 2> /dev/null | cut -b3-"}"
  setopt localoptions pipefail no_aliases 2> /dev/null
  REPORTTIME_=$REPORTTIME
  unset REPORTTIME
  local dir="$(eval "$cmd" | SKIM_DEFAULT_OPTIONS="--height ${SKIM_TMUX_HEIGHT:-40%} --reverse $SKIM_DEFAULT_OPTIONS $SKIM_ALT_C_OPTS" $(__skimcmd) --no-multi)"
  if ! [ -z $REPORTTIME_ ]; then
      REPORTTIME=$REPORTTIME_
  fi
  unset REPORTTIME_
  if [[ -z "$dir" ]]; then
    zle redisplay
    return 0
  fi
  if [ -z "$BUFFER" ]; then
    BUFFER="cd ${(q)dir}"
    zle accept-line
  else
    print -sr "cd ${(q)dir}"
    cd "$dir"
  fi
  local ret=$?
  unset dir # ensure this doesn't end up appearing in prompt expansion
  zle skim-redraw-prompt
  return $ret
}
zle     -N    skim-cd-widget
bindkey '\ec' skim-cd-widget

# CTRL-R - Paste the selected command from history into the command line
skim-history-widget() {
  local selected num
  setopt localoptions noglobsubst noposixbuiltins pipefail no_aliases 2> /dev/null
  local awk_filter='{ cmd=$0; sub(/^\s*[0-9]+\**\s+/, "", cmd); if (!seen[cmd]++) print $0 }'  # filter out duplicates
  local n=2 fc_opts=''
  if [[ -o extended_history ]]; then
    local today=$(date +%Y-%m-%d)
    # For today's commands, replace date ($2) with "today", otherwise remove time ($3).
    # And filter out duplicates.
    awk_filter='{
      if ($2 == "'$today'") sub($2 " ", "today'\''")
      else sub($3, "")
      line=$0; $1=""; $2=""; $3=""
      if (!seen[$0]++) print line
    }'
    fc_opts='-i'
    n=3
  fi
  selected=( $(fc -rl $fc_opts 1 | awk "$awk_filter" |
    SKIM_DEFAULT_OPTIONS="--height ${SKIM_TMUX_HEIGHT:-40%} $SKIM_DEFAULT_OPTIONS -n$n..,.. --tiebreak=index --bind=ctrl-r:toggle-sort $SKIM_CTRL_R_OPTS --query=${(qqq)LBUFFER} --no-multi" $(__skimcmd)) )
  local ret=$?
  if [ -n "$selected" ]; then
    num=$selected[1]
    if [ -n "$num" ]; then
      zle vi-fetch-history -n $num
    fi
  fi
  zle reset-prompt
  return $ret
}
zle     -N   skim-history-widget
bindkey '^R' skim-history-widget

} always {
  eval $__skim_key_bindings_options
  'unset' '__skim_key_bindings_options'
}
