#!/bin/fish
# completion.fish
# copied and modified from https://github.com/junegunn/fzf/blob/master/shell/key-bindings.fish
#     ____      ____
#    / __/___  / __/
#   / /_/_  / / /_
#  / __/ / /_/ __/
# /_/   /___/_/ key-bindings.fish
#
# - $SKIM_TMUX_OPTS
# - $SKIM_CTRL_T_COMMAND
# - $SKIM_CTRL_T_OPTS
# - $SKIM_CTRL_R_OPTS
# - $SKIM_ALT_C_COMMAND
# - $SKIM_ALT_C_OPTS

# Key bindings
# ------------
function skim_key_bindings

  # Store current token in $dir as root for the 'find' command
  function skim-file-widget -d "List files and folders"
    set -l commandline (__skim_parse_commandline)
    set -l dir $commandline[1]
    set -l skim_query $commandline[2]

    # "-path \$dir'*/\\.*'" matches hidden files/folders inside $dir but not
    # $dir itself, even if hidden.
    test -n "$SKIM_CTRL_T_COMMAND"; or set -l SKIM_CTRL_T_COMMAND "
    command find -L \$dir -mindepth 1 \\( -path \$dir'*/\\.*' -o -fstype 'sysfs' -o -fstype 'devfs' -o -fstype 'devtmpfs' \\) -prune \
    -o -type f -print \
    -o -type d -print \
    -o -type l -print 2> /dev/null | sed 's@^\./@@'"

    test -n "$SKIM_TMUX_HEIGHT"; or set SKIM_TMUX_HEIGHT 40%
    begin
      set -lx SKIM_DEFAULT_OPTIONS "--height $SKIM_TMUX_HEIGHT --reverse $SKIM_DEFAULT_OPTIONS $SKIM_CTRL_T_OPTS"
      eval "$SKIM_CTRL_T_COMMAND | "(__skimcmd)' -m --query "'$skim_query'"' | while read -l r; set result $result $r; end
    end
    if [ -z "$result" ]
      commandline -f repaint
      return
    else
      # Remove last token from commandline.
      commandline -t ""
    end
    for i in $result
      commandline -it -- (string escape $i)
      commandline -it -- ' '
    end
    commandline -f repaint
  end

  function skim-history-widget -d "Show command history"
    test -n "$SKIM_TMUX_HEIGHT"; or set SKIM_TMUX_HEIGHT 40%
    begin
      set -lx SKIM_DEFAULT_OPTIONS "--height $SKIM_TMUX_HEIGHT $SKIM_DEFAULT_OPTIONS --tiebreak=index --bind=ctrl-r:toggle-sort $SKIM_CTRL_R_OPTS --no-multi"

      set -l FISH_MAJOR (echo $version | cut -f1 -d.)
      set -l FISH_MINOR (echo $version | cut -f2 -d.)

      # history's -z flag is needed for multi-line support.
      # history's -z flag was added in fish 2.4.0, so don't use it for versions
      # before 2.4.0.
      if [ "$FISH_MAJOR" -gt 2 -o \( "$FISH_MAJOR" -eq 2 -a "$FISH_MINOR" -ge 4 \) ];
        history -z | eval (__skimcmd) --read0 --print0 -q '(commandline)' | read -lz result
        and commandline -- $result
      else
        history | eval (__skimcmd) -q '(commandline)' | read -l result
        and commandline -- $result
      end
    end
    commandline -f repaint
  end

  function skim-cd-widget -d "Change directory"
    set -l commandline (__skim_parse_commandline)
    set -l dir $commandline[1]
    set -l skim_query $commandline[2]

    test -n "$SKIM_ALT_C_COMMAND"; or set -l SKIM_ALT_C_COMMAND "
    command find -L \$dir -mindepth 1 \\( -path \$dir'*/\\.*' -o -fstype 'sysfs' -o -fstype 'devfs' -o -fstype 'devtmpfs' \\) -prune \
    -o -type d -print 2> /dev/null | sed 's@^\./@@'"
    test -n "$SKIM_TMUX_HEIGHT"; or set SKIM_TMUX_HEIGHT 40%
    begin
      set -lx SKIM_DEFAULT_OPTIONS "--height $SKIM_TMUX_HEIGHT --reverse $SKIM_DEFAULT_OPTIONS $SKIM_ALT_C_OPTS"
      eval "$SKIM_ALT_C_COMMAND | "(__skimcmd)' --no-multi --query "'$skim_query'"' | read -l result

      if [ -n "$result" ]
        cd $result

        # Remove last token from commandline.
        commandline -t ""
      end
    end

    commandline -f repaint
  end

  function __skimcmd
    test -n "$SKIM_TMUX"; or set SKIM_TMUX 0
    test -n "$SKIM_TMUX_HEIGHT"; or set SKIM_TMUX_HEIGHT 40%
    if [ -n "$SKIM_TMUX_OPTS" ]
      echo "sk-tmux $SKIM_TMUX_OPTS -- "
    else if [ $SKIM_TMUX -eq 1 ]
      echo "sk-tmux -d$SKIM_TMUX_HEIGHT -- "
    else
      echo "sk"
    end
  end

  bind \ct skim-file-widget
  bind \cr skim-history-widget
  bind \ec skim-cd-widget

  if bind -M insert > /dev/null 2>&1
    bind -M insert \ct skim-file-widget
    bind -M insert \cr skim-history-widget
    bind -M insert \ec skim-cd-widget
  end

  function __skim_parse_commandline -d 'Parse the current command line token and return split of existing filepath and rest of token'
    # eval is used to do shell expansion on paths
    set -l commandline (eval "printf '%s' "(commandline -t))

    if [ -z $commandline ]
      # Default to current directory with no --query
      set dir '.'
      set skim_query ''
    else
      set dir (__skim_get_dir $commandline)

      if [ "$dir" = "." -a (string sub -l 1 -- $commandline) != '.' ]
        # if $dir is "." but commandline is not a relative path, this means no file path found
        set skim_query $commandline
      else
        # Also remove trailing slash after dir, to "split" input properly
        set skim_query (string replace -r "^$dir/?" -- '' "$commandline")
      end
    end

    echo $dir
    echo $skim_query
  end

  function __skim_get_dir -d 'Find the longest existing filepath from input string'
    set dir $argv

    # Strip all trailing slashes. Ignore if $dir is root dir (/)
    if [ (string length -- $dir) -gt 1 ]
      set dir (string replace -r '/*$' -- '' $dir)
    end

    # Iteratively check if dir exists and strip tail end of path
    while [ ! -d "$dir" ]
      # If path is absolute, this can keep going until ends up at /
      # If path is relative, this can keep going until entire input is consumed, dirname returns "."
      set dir (dirname -- "$dir")
    end

    echo $dir
  end

end
