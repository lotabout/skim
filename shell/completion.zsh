#     ____      ____
#    / __/___  / __/
#   / /_/_  / / /_
#  / __/ / /_/ __/
# /_/   /___/_/-completion.zsh
#
# - $SKIM_TMUX               (default: 0)
# - $SKIM_TMUX_HEIGHT        (default: '40%')
# - $SKIM_COMPLETION_TRIGGER (default: '**')
# - $SKIM_COMPLETION_OPTS    (default: empty)

if [[ $- =~ i ]]; then

# To use custom commands instead of find, override _skim_compgen_{path,dir}
if ! declare -f _skim_compgen_path > /dev/null; then
  _skim_compgen_path() {
    echo "$1"
    command find -L "$1" \
      -name .git -prune -o -name .hg -prune -o -name .svn -prune -o \( -type d -o -type f -o -type l \) \
      -a -not -path "$1" -print 2> /dev/null | sed 's@^\./@@'
  }
fi

if ! declare -f _skim_compgen_dir > /dev/null; then
  _skim_compgen_dir() {
    command find -L "$1" \
      -name .git -prune -o -name .hg -prune -o -name .svn -prune -o -type d \
      -a -not -path "$1" -print 2> /dev/null | sed 's@^\./@@'
  }
fi

###########################################################

__skim_comprun() {
  if [[ "$(type _skim_comprun 2>&1)" =~ function ]]; then
    _skim_comprun "$@"
  elif [ -n "$TMUX_PANE" ] && [ "${SKIM_TMUX:-0}" != 0 ] && [ ${LINES:-40} -gt 15 ]; then
    shift
    sk-tmux -d "${SKIM_TMUX_HEIGHT:-40%}" "$@"
  else
    shift
    sk "$@"
  fi
}

# Extract the name of the command. e.g. foo=1 bar baz**<tab>
__skim_extract_command() {
  local token tokens
  tokens=(${(z)1})
  for token in $tokens; do
    if [[ "$token" =~ [[:alnum:]] && ! "$token" =~ "=" ]]; then
      echo "$token"
      return
    fi
  done
  echo "${tokens[1]}"
}

__skim_generic_path_completion() {
  local base lbuf cmd compgen skim_opts suffix tail dir leftover matches
  base=$1
  lbuf=$2
  cmd=$(__skim_extract_command "$lbuf")
  compgen=$3
  skim_opts=$4
  suffix=$5
  tail=$6

  setopt localoptions nonomatch
  eval "base=$base"
  [[ $base = *"/"* ]] && dir="$base"
  while [ 1 ]; do
    if [[ -z "$dir" || -d ${dir} ]]; then
      leftover=${base/#"$dir"}
      leftover=${leftover/#\/}
      [ -z "$dir" ] && dir='.'
      [ "$dir" != "/" ] && dir="${dir/%\//}"
      matches=$(eval "$compgen $(printf %q "$dir")" | SKIM_DEFAULT_OPTIONS="--height ${SKIM_TMUX_HEIGHT:-40%} --reverse $SKIM_DEFAULT_OPTIONS $SKIM_COMPLETION_OPTS" __skim_comprun "$cmd" ${(Q)${(Z+n+)skim_opts}} -q "$leftover" | while read item; do
        echo -n "${(q)item}$suffix "
      done)
      matches=${matches% }
      if [ -n "$matches" ]; then
        LBUFFER="$lbuf$matches$tail"
      fi
      zle reset-prompt
      break
    fi
    dir=$(dirname "$dir")
    dir=${dir%/}/
  done
}

_skim_path_completion() {
  __skim_generic_path_completion "$1" "$2" _skim_compgen_path \
    "-m" "" " "
}

_skim_dir_completion() {
  __skim_generic_path_completion "$1" "$2" _skim_compgen_dir \
    "" "/" ""
}

_skim_feed_fifo() (
  command rm -f "$1"
  mkfifo "$1"
  cat <&0 > "$1" &
)

_skim_complete() {
  local fifo skim_opts lbuf cmd matches post
  fifo="${TMPDIR:-/tmp}/skim-complete-fifo-$$"
  skim_opts=$1
  lbuf=$2
  cmd=$(__skim_extract_command "$lbuf")
  post="${funcstack[2]}_post"
  type $post > /dev/null 2>&1 || post=cat

  _skim_feed_fifo "$fifo"
  matches=$(cat "$fifo" | SKIM_DEFAULT_OPTIONS="--height ${SKIM_TMUX_HEIGHT:-40%} --reverse $SKIM_DEFAULT_OPTIONS $SKIM_COMPLETION_OPTS" __skim_comprun "$cmd" ${(Q)${(Z+n+)skim_opts}} -q "${(Q)prefix}" | $post | tr '\n' ' ')
  if [ -n "$matches" ]; then
    LBUFFER="$lbuf$matches"
  fi
  zle reset-prompt
  command rm -f "$fifo"
}

_skim_complete_telnet() {
  _skim_complete '-m' "$@" < <(
    command grep -v '^\s*\(#\|$\)' /etc/hosts | command grep -Fv '0.0.0.0' |
        awk '{if (length($2) > 0) {print $2}}' | sort -u
  )
}

_skim_complete_ssh() {
  _skim_complete '-m' "$@" < <(
    setopt localoptions nonomatch
    command cat <(cat ~/.ssh/config ~/.ssh/config.d/* /etc/ssh/ssh_config 2> /dev/null | command grep -i '^\s*host\(name\)\? ' | awk '{for (i = 2; i <= NF; i++) print $1 " " $i}' | command grep -v '[*?]') \
        <(command grep -oE '^[[a-z0-9.,:-]+' ~/.ssh/known_hosts | tr ',' '\n' | tr -d '[' | awk '{ print $1 " " $1 }') \
        <(command grep -v '^\s*\(#\|$\)' /etc/hosts | command grep -Fv '0.0.0.0') |
        awk '{if (length($2) > 0) {print $2}}' | sort -u
  )
}

_skim_complete_export() {
  _skim_complete '-m' "$@" < <(
    declare -xp | sed 's/=.*//' | sed 's/.* //'
  )
}

_skim_complete_unset() {
  _skim_complete '-m' "$@" < <(
    declare -xp | sed 's/=.*//' | sed 's/.* //'
  )
}

_skim_complete_unalias() {
  _skim_complete '-m' "$@" < <(
    alias | sed 's/=.*//'
  )
}

skim-completion() {
  local tokens cmd prefix trigger tail matches lbuf d_cmds
  setopt localoptions noshwordsplit noksh_arrays noposixbuiltins

  # http://zsh.sourceforge.net/FAQ/zshfaq03.html
  # http://zsh.sourceforge.net/Doc/Release/Expansion.html#Parameter-Expansion-Flags
  tokens=(${(z)LBUFFER})
  if [ ${#tokens} -lt 1 ]; then
    zle ${skim_default_completion:-expand-or-complete}
    return
  fi

  cmd=$(__skim_extract_command "$LBUFFER")

  # Explicitly allow for empty trigger.
  trigger=${SKIM_COMPLETION_TRIGGER-'**'}
  [ -z "$trigger" -a ${LBUFFER[-1]} = ' ' ] && tokens+=("")

  # When the trigger starts with ';', it becomes a separate token
  if [[ ${LBUFFER} = *"${tokens[-2]}${tokens[-1]}" ]]; then
    tokens[-2]="${tokens[-2]}${tokens[-1]}"
    tokens=(${tokens[0,-2]})
  fi

  tail=${LBUFFER:$(( ${#LBUFFER} - ${#trigger} ))}
  # Kill completion (do not require trigger sequence)
  if [ $cmd = kill -a ${LBUFFER[-1]} = ' ' ]; then
    matches=$(command ps -ef | sed 1d | SKIM_DEFAULT_OPTIONS="--height ${SKIM_TMUX_HEIGHT:-50%} --min-height 15 --reverse $SKIM_DEFAULT_OPTIONS $SKIM_COMPLETION_OPTS --preview 'echo {}' --preview-window down:3:wrap" __skim_comprun "$cmd" -m | awk '{print $2}' | tr '\n' ' ')
    if [ -n "$matches" ]; then
      LBUFFER="$LBUFFER$matches"
    fi
    zle reset-prompt
  # Trigger sequence given
  elif [ ${#tokens} -gt 1 -a "$tail" = "$trigger" ]; then
    d_cmds=(${=SKIM_COMPLETION_DIR_COMMANDS:-cd pushd rmdir})

    [ -z "$trigger"      ] && prefix=${tokens[-1]} || prefix=${tokens[-1]:0:-${#trigger}}
    [ -z "${tokens[-1]}" ] && lbuf=$LBUFFER        || lbuf=${LBUFFER:0:-${#tokens[-1]}}

    if eval "type _skim_complete_${cmd} > /dev/null"; then
      prefix="$prefix" eval _skim_complete_${cmd} ${(q)lbuf}
    elif [ ${d_cmds[(i)$cmd]} -le ${#d_cmds} ]; then
      _skim_dir_completion "$prefix" "$lbuf"
    else
      _skim_path_completion "$prefix" "$lbuf"
    fi
  # Fall back to default completion
  else
    zle ${skim_default_completion:-expand-or-complete}
  fi
}

[ -z "$skim_default_completion" ] && {
  binding=$(bindkey '^I')
  [[ $binding =~ 'undefined-key' ]] || skim_default_completion=$binding[(s: :w)2]
  unset binding
}

zle     -N   skim-completion
bindkey '^I' skim-completion

fi
