" Copyright (c) 2017 Junegunn Choi
"
" MIT License
"
" Permission is hereby granted, free of charge, to any person obtaining
" a copy of this software and associated documentation files (the
" "Software"), to deal in the Software without restriction, including
" without limitation the rights to use, copy, modify, merge, publish,
" distribute, sublicense, and/or sell copies of the Software, and to
" permit persons to whom the Software is furnished to do so, subject to
" the following conditions:
"
" The above copyright notice and this permission notice shall be
" included in all copies or substantial portions of the Software.
"
" THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
" EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
" MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
" NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE
" LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
" OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION
" WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

if exists('g:loaded_skim')
  finish
endif
let g:loaded_skim = 1

if empty($SKIM_DEFAULT_COMMAND)
    let $SKIM_DEFAULT_COMMAND = "fd --type f || git ls-tree -r --name-only HEAD || rg --files || ag -l -g \"\" || find ."
endif

let s:is_win = has('win32') || has('win64')
if s:is_win && &shellslash
  set noshellslash
  let s:base_dir = expand('<sfile>:h:h')
  set shellslash
else
  let s:base_dir = expand('<sfile>:h:h')
endif
if s:is_win
  let s:term_marker = '&::SKIM'

  function! s:skim_call(fn, ...)
    let shellslash = &shellslash
    try
      set noshellslash
      return call(a:fn, a:000)
    finally
      let &shellslash = shellslash
    endtry
  endfunction

  " Use utf-8 for skim.vim commands
  " Return array of shell commands for cmd.exe
  function! s:enc_to_cp(str)
    if !has('iconv')
      return a:str
    endif
    if !exists('s:codepage')
      let s:codepage = libcallnr('kernel32.dll', 'GetACP', 0)
    endif
    return iconv(a:str, &encoding, 'cp'.s:codepage)
  endfunction
  function! s:wrap_cmds(cmds)
    return map([
      \ '@echo off',
      \ 'setlocal enabledelayedexpansion']
    \ + (has('gui_running') ? ['set TERM= > nul'] : [])
    \ + (type(a:cmds) == type([]) ? a:cmds : [a:cmds])
    \ + ['endlocal'],
    \ '<SID>enc_to_cp(v:val."\r")')
  endfunction
else
  let s:term_marker = ";#SKIM"

  function! s:skim_call(fn, ...)
    return call(a:fn, a:000)
  endfunction

  function! s:wrap_cmds(cmds)
    return a:cmds
  endfunction

  function! s:enc_to_cp(str)
    return a:str
  endfunction
endif

function! s:shellesc_cmd(arg)
  let escaped = substitute(a:arg, '[&|<>()@^]', '^&', 'g')
  let escaped = substitute(escaped, '%', '%%', 'g')
  let escaped = substitute(escaped, '"', '\\^&', 'g')
  let escaped = substitute(escaped, '\(\\\+\)\(\\^\)', '\1\1\2', 'g')
  return '^"'.substitute(escaped, '\(\\\+\)$', '\1\1', '').'^"'
endfunction

function! skim#shellescape(arg, ...)
  let shell = get(a:000, 0, s:is_win ? 'cmd.exe' : 'sh')
  if shell =~# 'cmd.exe$'
    return s:shellesc_cmd(a:arg)
  endif
  return s:skim_call('shellescape', a:arg)
endfunction

function! s:skim_getcwd()
  return s:skim_call('getcwd')
endfunction

function! s:skim_fnamemodify(fname, mods)
  return s:skim_call('fnamemodify', a:fname, a:mods)
endfunction

function! s:skim_expand(fmt)
  return s:skim_call('expand', a:fmt, 1)
endfunction

function! s:skim_tempname()
  return s:skim_call('tempname')
endfunction

let s:layout_keys = ['window', 'tmux', 'up', 'down', 'left', 'right']
let s:skim_rs = s:base_dir.'/bin/sk'
let s:skim_tmux = s:base_dir.'/bin/sk-tmux'

let s:cpo_save = &cpo
set cpo&vim

function! s:popup_support()
  return has('nvim') ? has('nvim-0.4') : has('popupwin') && has('patch-8.2.191')
endfunction

function! s:default_layout()
  return s:popup_support()
        \ ? { 'window' : { 'width': 0.9, 'height': 0.8, 'highlight': 'Normal' } }
        \ : { 'down': '~40%' }
endfunction

function! skim#install()
  if s:is_win && !has('win32unix')
    let script = s:base_dir.'/install.ps1'
    if !filereadable(script)
      throw script.' not found'
    endif
    let script = 'powershell -ExecutionPolicy Bypass -file ' . script
  else
    let script = s:base_dir.'/install'
    if !executable(script)
      throw script.' not found'
    endif
    let script .= ' --bin'
  endif

  call s:warn('Running skim installer ...')
  call system(script)
  if v:shell_error
    throw 'Failed to download skim: '.script
  endif
endfunction

function! skim#exec()
  if !exists('s:exec')
    if executable(s:skim_rs)
      let s:exec = s:skim_rs
    elseif executable('sk')
      let s:exec = 'sk'
    elseif input('skim executable not found. Download binary? (y/n) ') =~? '^y'
      redraw
      call skim#install()
      return skim#exec()
    else
      redraw
      throw 'skim executable not found'
    endif
  endif
  return s:exec
endfunction

function! s:tmux_enabled()
  if has('gui_running') || !exists('$TMUX')
    return 0
  endif

  if exists('s:tmux')
    return s:tmux
  endif

  let s:tmux = 0
  if !executable(s:skim_tmux)
    if executable('sk-tmux')
      let s:skim_tmux = 'sk-tmux'
    else
      return 0
    endif
  endif

  let output = system('tmux -V')
  let s:tmux = !v:shell_error && output >= 'tmux 1.7'
  return s:tmux
endfunction

function! s:escape(path)
  let path = fnameescape(a:path)
  return s:is_win ? escape(path, '$') : path
endfunction

function! s:error(msg)
  echohl ErrorMsg
  echom a:msg
  echohl None
endfunction

function! s:warn(msg)
  echohl WarningMsg
  echom a:msg
  echohl None
endfunction

function! s:has_any(dict, keys)
  for key in a:keys
    if has_key(a:dict, key)
      return 1
    endif
  endfor
  return 0
endfunction

function! s:open(cmd, target)
  if stridx('edit', a:cmd) == 0 && s:skim_fnamemodify(a:target, ':p') ==# s:skim_expand('%:p')
    return
  endif
  execute a:cmd s:escape(a:target)
endfunction

function! s:common_sink(action, lines) abort
  if len(a:lines) < 2
    return
  endif
  let key = remove(a:lines, 0)
  let Cmd = get(a:action, key, 'e')
  if type(Cmd) == type(function('call'))
    return Cmd(a:lines)
  endif
  if len(a:lines) > 1
    augroup skim_swap
      autocmd SwapExists * let v:swapchoice='o'
            \| call s:warn('skim: E325: swap file exists: '.s:skim_expand('<afile>'))
    augroup END
  endif
  try
    let empty = empty(s:skim_expand('%')) && line('$') == 1 && empty(getline(1)) && !&modified
    " Preserve the current working directory in case it's changed during
    " the execution (e.g. `set autochdir` or `autocmd BufEnter * lcd ...`)
    let cwd = exists('w:skim_pushd') ? w:skim_pushd.dir : expand('%:p:h')
    for item in a:lines
      if item[0] != '~' && item !~ (s:is_win ? '^[A-Z]:\' : '^/')
        let item = join([cwd, item], (s:is_win ? '\' : '/'))
      endif
      if empty
        execute 'e' s:escape(item)
        let empty = 0
      else
        call s:open(Cmd, item)
      endif
      if !has('patch-8.0.0177') && !has('nvim-0.2') && exists('#BufEnter')
            \ && isdirectory(item)
        doautocmd BufEnter
      endif
    endfor
  catch /^Vim:Interrupt$/
  finally
    silent! autocmd! skim_swap
  endtry
endfunction

function! s:get_color(attr, ...)
  let gui = !s:is_win && !has('win32unix') && has('termguicolors') && &termguicolors
  let fam = gui ? 'gui' : 'cterm'
  let pat = gui ? '^#[a-f0-9]\+' : '^[0-9]\+$'
  for group in a:000
    let code = synIDattr(synIDtrans(hlID(group)), a:attr, fam)
    if code =~? pat
      return code
    endif
  endfor
  return ''
endfunction

function! s:defaults()
  let rules = copy(get(g:, 'skim_colors', {}))
  let colors = join(map(items(filter(map(rules, 'call("s:get_color", v:val)'), '!empty(v:val)')), 'join(v:val, ":")'), ',')
  return empty(colors) ? '' : skim#shellescape('--color='.colors)
endfunction

function! s:validate_layout(layout)
  for key in keys(a:layout)
    if index(s:layout_keys, key) < 0
      throw printf('Invalid entry in g:skim_layout: %s (allowed: %s)%s',
            \ key, join(s:layout_keys, ', '), key == 'options' ? '. Use $SKIM_DEFAULT_OPTIONS.' : '')
    endif
  endfor
  return a:layout
endfunction

function! s:evaluate_opts(options)
  return type(a:options) == type([]) ?
        \ join(map(copy(a:options), 'skim#shellescape(v:val)')) : a:options
endfunction

" [name string,] [opts dict,] [fullscreen boolean]
function! skim#wrap(...)
  let args = ['', {}, 0]
  let expects = map(copy(args), 'type(v:val)')
  let tidx = 0
  for arg in copy(a:000)
    let tidx = index(expects, type(arg) == 6 ? type(0) : type(arg), tidx)
    if tidx < 0
      throw 'Invalid arguments (expected: [name string] [opts dict] [fullscreen boolean])'
    endif
    let args[tidx] = arg
    let tidx += 1
    unlet arg
  endfor
  let [name, opts, bang] = args

  if len(name)
    let opts.name = name
  end

  " Layout: g:skim_layout (and deprecated g:skim_height)
  if bang
    for key in s:layout_keys
      if has_key(opts, key)
        call remove(opts, key)
      endif
    endfor
  elseif !s:has_any(opts, s:layout_keys)
    if !exists('g:skim_layout') && exists('g:skim_height')
      let opts.down = g:skim_height
    else
      let opts = extend(opts, s:validate_layout(get(g:, 'skim_layout', s:default_layout())))
    endif
  endif

  " Interactive commands should use --cmd-history for query history
  let history_option = '--history'
  if index(opts['options'], '-i') != -1
    let history_option = '--cmd-history'
  endif

  " Colors: g:skim_colors
  let opts.options = s:defaults() .' '. s:evaluate_opts(get(opts, 'options', ''))

  " History: g:skim_history_dir
  if len(name) && len(get(g:, 'skim_history_dir', ''))
    let dir = s:skim_expand(g:skim_history_dir)
    if !isdirectory(dir)
      call mkdir(dir, 'p')
    endif
    let history = skim#shellescape(dir.'/'.name)
    let opts.options = join([history_option, history, opts.options])
  endif

  " Action: g:skim_action
  if !s:has_any(opts, ['sink', 'sink*'])
    let opts._action = get(g:, 'skim_action', s:default_action)
    let opts.options .= ' --expect='.join(keys(opts._action), ',')
    function! opts.sink(lines) abort
      return s:common_sink(self._action, a:lines)
    endfunction
    let opts['sink*'] = remove(opts, 'sink')
  endif

  return opts
endfunction

function! s:use_sh()
  let [shell, shellslash, shellcmdflag, shellxquote] = [&shell, &shellslash, &shellcmdflag, &shellxquote]
  if s:is_win
    set shell=cmd.exe
    set noshellslash
    let &shellcmdflag = has('nvim') ? '/s /c' : '/c'
    let &shellxquote = has('nvim') ? '"' : '('
  else
    set shell=sh
  endif
  return [shell, shellslash, shellcmdflag, shellxquote]
endfunction

function! skim#run(...) abort
try
  let [shell, shellslash, shellcmdflag, shellxquote] = s:use_sh()

  let dict   = exists('a:1') ? copy(a:1) : {}
  let temps  = { 'result': s:skim_tempname() }
  let optstr = s:evaluate_opts(get(dict, 'options', ''))
  try
    let skim_exec = skim#shellescape(skim#exec())
  catch
    throw v:exception
  endtry

  if !has_key(dict, 'dir')
    let dict.dir = s:skim_getcwd()
  endif
  if has('win32unix') && has_key(dict, 'dir')
    let dict.dir = fnamemodify(dict.dir, ':p')
  endif

  if has_key(dict, 'source')
    let source = dict.source
    let type = type(source)
    if type == 1
      let prefix = '( '.source.' )|'
    elseif type == 3
      let temps.input = s:skim_tempname()
      call writefile(map(source, '<SID>enc_to_cp(v:val)'), temps.input)
      let prefix = (s:is_win ? 'type ' : 'cat ').skim#shellescape(temps.input).'|'
    else
      throw 'Invalid source type'
    endif
  else
    let prefix = ''
  endif

  let prefer_tmux = get(g:, 'skim_prefer_tmux', 0) || has_key(dict, 'tmux')
  let use_height = has_key(dict, 'down') && !has('gui_running') &&
        \ !(has('nvim') || s:is_win || has('win32unix') || s:present(dict, 'up', 'left', 'right', 'window')) &&
        \ executable('tput') && filereadable('/dev/tty')
  let has_vim8_term = has('terminal') && has('patch-8.0.995')
  let has_nvim_term = has('nvim-0.2.1') || has('nvim') && !s:is_win
  let use_term = has_nvim_term ||
    \ has_vim8_term && !has('win32unix') && (has('gui_running') || s:is_win || !use_height && s:present(dict, 'down', 'up', 'left', 'right', 'window'))
  let use_tmux = (has_key(dict, 'tmux') || (!use_height && !use_term || prefer_tmux) && !has('win32unix') && s:splittable(dict)) && s:tmux_enabled()
  if prefer_tmux && use_tmux
    let use_height = 0
    let use_term = 0
  endif
  if use_height
    let height = s:calc_size(&lines, dict.down, dict)
    let optstr .= ' --height='.height
  elseif use_term
    let optstr .= ' --no-height'
  endif
  let command = prefix.(use_tmux ? s:skim_tmux(dict) : skim_exec).' '.optstr.' > '.temps.result

  if use_term
    return s:execute_term(dict, command, temps)
  endif

  let lines = use_tmux ? s:execute_tmux(dict, command, temps)
                 \ : s:execute(dict, command, use_height, temps)
  call s:callback(dict, lines)
  return lines
finally
  let [&shell, &shellslash, &shellcmdflag, &shellxquote] = [shell, shellslash, shellcmdflag, shellxquote]
endtry
endfunction

function! s:present(dict, ...)
  for key in a:000
    if !empty(get(a:dict, key, ''))
      return 1
    endif
  endfor
  return 0
endfunction

function! s:skim_tmux(dict)
  let size = get(a:dict, 'tmux', '')
  if empty(size)
    for o in ['up', 'down', 'left', 'right']
      if s:present(a:dict, o)
        let spec = a:dict[o]
        if (o == 'up' || o == 'down') && spec[0] == '~'
          let size = '-'.o[0].s:calc_size(&lines, spec, a:dict)
        else
          " Legacy boolean option
          let size = '-'.o[0].(spec == 1 ? '' : substitute(spec, '^\~', '', ''))
        endif
        break
      endif
    endfor
  endif
  return printf('LINES=%d COLUMNS=%d %s %s %s --',
    \ &lines, &columns, skim#shellescape(s:skim_tmux), size, (has_key(a:dict, 'source') ? '' : '-'))
endfunction

function! s:splittable(dict)
  return s:present(a:dict, 'up', 'down') && &lines > 15 ||
        \ s:present(a:dict, 'left', 'right') && &columns > 40
endfunction

function! s:pushd(dict)
  if s:present(a:dict, 'dir')
    let cwd = s:skim_getcwd()
    let w:skim_pushd = {
    \   'command': haslocaldir() ? 'lcd' : (exists(':tcd') && haslocaldir(-1) ? 'tcd' : 'cd'),
    \   'origin': cwd,
    \   'bufname': bufname('')
    \ }
    execute 'lcd' s:escape(a:dict.dir)
    let cwd = s:skim_getcwd()
    let w:skim_pushd.dir = cwd
    let a:dict.pushd = w:skim_pushd
    return cwd
  endif
  return ''
endfunction

augroup skim_popd
  autocmd!
  autocmd WinEnter * call s:dopopd()
augroup END

function! s:dopopd()
  if !exists('w:skim_pushd')
    return
  endif

  " FIXME: We temporarily change the working directory to 'dir' entry
  " of options dictionary (set to the current working directory if not given)
  " before running skim.
  "
  " e.g. call skim#run({'dir': '/tmp', 'source': 'ls', 'sink': 'e'})
  "
  " After processing the sink function, we have to restore the current working
  " directory. But doing so may not be desirable if the function changed the
  " working directory on purpose.
  "
  " So how can we tell if we should do it or not? A simple heuristic we use
  " here is that we change directory only if the current working directory
  " matches 'dir' entry. However, it is possible that the sink function did
  " change the directory to 'dir'. In that case, the user will have an
  " unexpected result.
  if s:skim_getcwd() ==# w:skim_pushd.dir && (!&autochdir || w:skim_pushd.bufname ==# bufname(''))
    execute w:skim_pushd.command s:escape(w:skim_pushd.origin)
  endif
  unlet! w:skim_pushd
endfunction

function! s:xterm_launcher()
  let fmt = 'xterm -T "[skim]" -bg "%s" -fg "%s" -geometry %dx%d+%d+%d -e bash -ic %%s'
  if has('gui_macvim')
    let fmt .= '&& osascript -e "tell application \"MacVim\" to activate"'
  endif
  return printf(fmt,
    \ escape(synIDattr(hlID("Normal"), "bg"), '#'), escape(synIDattr(hlID("Normal"), "fg"), '#'),
    \ &columns, &lines/2, getwinposx(), getwinposy())
endfunction
unlet! s:launcher
if s:is_win || has('win32unix')
  let s:launcher = '%s'
else
  let s:launcher = function('s:xterm_launcher')
endif

function! s:exit_handler(code, command, ...)
  if a:code == 130
    return 0
  elseif has('nvim') && a:code == 129
    " When deleting the terminal buffer while skim is still running,
    " Nvim sends SIGHUP.
    return 0
  elseif a:code > 1
    call s:error('Error running ' . a:command)
    if !empty(a:000)
      sleep
    endif
    return 0
  endif
  return 1
endfunction

function! s:execute(dict, command, use_height, temps) abort
  call s:pushd(a:dict)
  if has('unix') && !a:use_height
    silent! !clear 2> /dev/null
  endif
  let escaped = (a:use_height || s:is_win) ? a:command : escape(substitute(a:command, '\n', '\\n', 'g'), '%#!')
  if has('gui_running')
    let Launcher = get(a:dict, 'launcher', get(g:, 'Skim_launcher', get(g:, 'skim_launcher', s:launcher)))
    let fmt = type(Launcher) == 2 ? call(Launcher, []) : Launcher
    if has('unix')
      let escaped = "'".substitute(escaped, "'", "'\"'\"'", 'g')."'"
    endif
    let command = printf(fmt, escaped)
  else
    let command = escaped
  endif
  if s:is_win
    let batchfile = s:skim_tempname().'.bat'
    call writefile(s:wrap_cmds(command), batchfile)
    let command = batchfile
    let a:temps.batchfile = batchfile
    if has('nvim')
      let skim = {}
      let skim.dict = a:dict
      let skim.temps = a:temps
      function! skim.on_exit(job_id, exit_status, event) dict
        call s:pushd(self.dict)
        let lines = s:collect(self.temps)
        call s:callback(self.dict, lines)
      endfunction
      let cmd = 'start /wait cmd /c '.command
      call jobstart(cmd, skim)
      return []
    endif
  elseif has('win32unix') && $TERM !=# 'cygwin'
    let shellscript = s:skim_tempname()
    call writefile([command], shellscript)
    let command = 'cmd.exe /C '.skim#shellescape('set "TERM=" & start /WAIT sh -c '.shellscript)
    let a:temps.shellscript = shellscript
  endif
  if a:use_height
    let stdin = has_key(a:dict, 'source') ? '' : '< /dev/tty'
    call system(printf('tput cup %d > /dev/tty; tput cnorm > /dev/tty; %s %s 2> /dev/tty', &lines, command, stdin))
  else
    execute 'silent !'.command
  endif
  let exit_status = v:shell_error
  redraw!
  return s:exit_handler(exit_status, command) ? s:collect(a:temps) : []
endfunction

function! s:execute_tmux(dict, command, temps) abort
  let command = a:command
  let cwd = s:pushd(a:dict)
  if len(cwd)
    " -c '#{pane_current_path}' is only available on tmux 1.9 or above
    let command = join(['cd', skim#shellescape(cwd), '&&', command])
  endif

  call system(command)
  let exit_status = v:shell_error
  redraw!
  return s:exit_handler(exit_status, command) ? s:collect(a:temps) : []
endfunction

function! s:calc_size(max, val, dict)
  let val = substitute(a:val, '^\~', '', '')
  if val =~ '%$'
    let size = a:max * str2nr(val[:-2]) / 100
  else
    let size = min([a:max, str2nr(val)])
  endif

  let srcsz = -1
  if type(get(a:dict, 'source', 0)) == type([])
    let srcsz = len(a:dict.source)
  endif

  let opts = $SKIM_DEFAULT_OPTIONS.' '.s:evaluate_opts(get(a:dict, 'options', ''))
  if opts =~ 'preview'
    return size
  endif
  let margin = match(opts, '--inline-info\|--info[^-]\{-}inline') > match(opts, '--no-inline-info\|--info[^-]\{-}\(default\|hidden\)') ? 1 : 2
  let margin += stridx(opts, '--border') > stridx(opts, '--no-border') ? 2 : 0
  if stridx(opts, '--header') > stridx(opts, '--no-header')
    let margin += len(split(opts, "\n"))
  endif
  return srcsz >= 0 ? min([srcsz + margin, size]) : size
endfunction

function! s:getpos()
  return {'tab': tabpagenr(), 'win': winnr(), 'winid': win_getid(), 'cnt': winnr('$'), 'tcnt': tabpagenr('$')}
endfunction

function! s:split(dict)
  let directions = {
  \ 'up':    ['topleft', 'resize', &lines],
  \ 'down':  ['botright', 'resize', &lines],
  \ 'left':  ['vertical topleft', 'vertical resize', &columns],
  \ 'right': ['vertical botright', 'vertical resize', &columns] }
  let ppos = s:getpos()
  let is_popup = 0
  try
    if s:present(a:dict, 'window')
      if type(a:dict.window) == type({})
        if !s:popup_support()
          throw 'Nvim 0.4+ or Vim 8.2.191+ with popupwin feature is required for pop-up window'
        end
        call s:popup(a:dict.window)
        let is_popup = 1
      else
        execute 'keepalt' a:dict.window
      endif
    elseif !s:splittable(a:dict)
      execute (tabpagenr()-1).'tabnew'
    else
      for [dir, triple] in items(directions)
        let val = get(a:dict, dir, '')
        if !empty(val)
          let [cmd, resz, max] = triple
          if (dir == 'up' || dir == 'down') && val[0] == '~'
            let sz = s:calc_size(max, val, a:dict)
          else
            let sz = s:calc_size(max, val, {})
          endif
          execute cmd sz.'new'
          execute resz sz
          return [ppos, {}, is_popup]
        endif
      endfor
    endif
    return [ppos, is_popup ? {} : { '&l:wfw': &l:wfw, '&l:wfh': &l:wfh }, is_popup]
  finally
    if !is_popup
      setlocal winfixwidth winfixheight
    endif
  endtry
endfunction

function! s:execute_term(dict, command, temps) abort
  let winrest = winrestcmd()
  let pbuf = bufnr('')
  let [ppos, winopts, is_popup] = s:split(a:dict)
  call s:use_sh()
  let b:skim = a:dict
  let skim = { 'buf': bufnr(''), 'pbuf': pbuf, 'ppos': ppos, 'dict': a:dict, 'temps': a:temps,
            \ 'winopts': winopts, 'winrest': winrest, 'lines': &lines,
            \ 'columns': &columns, 'command': a:command }
  function! skim.switch_back(inplace)
    if a:inplace && bufnr('') == self.buf
      if bufexists(self.pbuf)
        execute 'keepalt b' self.pbuf
      endif
      " No other listed buffer
      if bufnr('') == self.buf
        enew
      endif
    endif
  endfunction
  function! skim.on_exit(id, code, ...)
    if s:getpos() == self.ppos " {'window': 'enew'}
      for [opt, val] in items(self.winopts)
        execute 'let' opt '=' val
      endfor
      call self.switch_back(1)
    else
      if bufnr('') == self.buf
        " We use close instead of bd! since Vim does not close the split when
        " there's no other listed buffer (nvim +'set nobuflisted')
        close
      endif
      silent! execute 'tabnext' self.ppos.tab
      silent! execute self.ppos.win.'wincmd w'
    endif

    if bufexists(self.buf)
      execute 'bd!' self.buf
    endif

    if &lines == self.lines && &columns == self.columns && s:getpos() == self.ppos
      execute self.winrest
    endif

    if !s:exit_handler(a:code, self.command, 1)
      return
    endif

    call s:pushd(self.dict)
    let lines = s:collect(self.temps)
    call s:callback(self.dict, lines)
    call self.switch_back(s:getpos() == self.ppos)
  endfunction

  try
    call s:pushd(a:dict)
    if s:is_win
      let skim.temps.batchfile = s:skim_tempname().'.bat'
      call writefile(s:wrap_cmds(a:command), skim.temps.batchfile)
      let command = skim.temps.batchfile
    else
      let command = a:command
    endif
    let command .= s:term_marker
    if has('nvim')
      call termopen(command, skim)
    else
      let term_opts = {'exit_cb': function(skim.on_exit)}
      if is_popup
        let term_opts.hidden = 1
      else
        let term_opts.curwin = 1
      endif
      let skim.buf = term_start([&shell, &shellcmdflag, command], term_opts)
      if is_popup && exists('#TerminalWinOpen')
        doautocmd <nomodeline> TerminalWinOpen
      endif
      if !has('patch-8.0.1261') && !s:is_win
        call term_wait(skim.buf, 20)
      endif
    endif
  finally
    call s:dopopd()
  endtry
  setlocal nospell bufhidden=wipe nobuflisted nonumber
  setf skim
  startinsert
  return []
endfunction

function! s:collect(temps) abort
  try
    return filereadable(a:temps.result) ? readfile(a:temps.result) : []
  finally
    for tf in values(a:temps)
      silent! call delete(tf)
    endfor
  endtry
endfunction

function! s:callback(dict, lines) abort
  let popd = has_key(a:dict, 'pushd')
  if popd
    let w:skim_pushd = a:dict.pushd
  endif

  try
    if has_key(a:dict, 'sink')
      for line in a:lines
        if type(a:dict.sink) == 2
          call a:dict.sink(line)
        else
          execute a:dict.sink s:escape(line)
        endif
      endfor
    endif
    if has_key(a:dict, 'sink*')
      call a:dict['sink*'](a:lines)
    endif
  catch
    if stridx(v:exception, ':E325:') < 0
      echoerr v:exception
    endif
  endtry

  " We may have opened a new window or tab
  if popd
    let w:skim_pushd = a:dict.pushd
    call s:dopopd()
  endif
endfunction

if has('nvim')
  function s:create_popup(hl, opts) abort
    let buf = nvim_create_buf(v:false, v:true)
    let opts = extend({'relative': 'editor', 'style': 'minimal'}, a:opts)
    let border = has_key(opts, 'border') ? remove(opts, 'border') : []
    let win = nvim_open_win(buf, v:true, opts)
    call setwinvar(win, '&winhighlight', 'NormalFloat:'..a:hl)
    call setwinvar(win, '&colorcolumn', '')
    if !empty(border)
      call nvim_buf_set_lines(buf, 0, -1, v:true, border)
    endif
    return buf
  endfunction
else
  function! s:create_popup(hl, opts) abort
    let is_frame = has_key(a:opts, 'border')
    let s:popup_create = {buf -> popup_create(buf, #{
      \ line: a:opts.row,
      \ col: a:opts.col,
      \ minwidth: a:opts.width,
      \ minheight: a:opts.height,
      \ zindex: 50 - is_frame,
    \ })}
    if is_frame
      let id = s:popup_create('')
      call setwinvar(id, '&wincolor', a:hl)
      call setbufline(winbufnr(id), 1, a:opts.border)
      execute 'autocmd BufWipeout * ++once call popup_close('..id..')'
      return winbufnr(id)
    else
      autocmd TerminalOpen * ++once call s:popup_create(str2nr(expand('<abuf>')))
    endif
  endfunction
endif

function! s:popup(opts) abort
  " Support ambiwidth == 'double'
  let ambidouble = &ambiwidth == 'double' ? 2 : 1

  " Size and position
  let width = min([max([8, a:opts.width > 1 ? a:opts.width : float2nr(&columns * a:opts.width)]), &columns])
  let width += width % ambidouble
  let height = min([max([4, a:opts.height > 1 ? a:opts.height : float2nr(&lines * a:opts.height)]), &lines - has('nvim')])
  let row = float2nr(get(a:opts, 'yoffset', 0.5) * (&lines - height))
  let col = float2nr(get(a:opts, 'xoffset', 0.5) * (&columns - width))

  " Managing the differences
  let row = min([max([0, row]), &lines - has('nvim') - height])
  let col = min([max([0, col]), &columns - width])
  let row += !has('nvim')
  let col += !has('nvim')

  " Border style
  let style = tolower(get(a:opts, 'border', 'rounded'))
  if !has_key(a:opts, 'border') && !get(a:opts, 'rounded', 1)
    let style = 'sharp'
  endif

  if style =~ 'vertical\|left\|right'
    let mid = style == 'vertical' ? '│' .. repeat(' ', width - 2 * ambidouble) .. '│' :
            \ style == 'left'     ? '│' .. repeat(' ', width - 1 * ambidouble)
            \                     :        repeat(' ', width - 1 * ambidouble) .. '│'
    let border = repeat([mid], height)
    let shift = { 'row': 0, 'col': style == 'right' ? 0 : 2, 'width': style == 'vertical' ? -4 : -2, 'height': 0 }
  elseif style =~ 'horizontal\|top\|bottom'
    let hor = repeat('─', width / ambidouble)
    let mid = repeat(' ', width)
    let border = style == 'horizontal' ? [hor] + repeat([mid], height - 2) + [hor] :
               \ style == 'top'        ? [hor] + repeat([mid], height - 1)
               \                       :         repeat([mid], height - 1) + [hor]
    let shift = { 'row': style == 'bottom' ? 0 : 1, 'col': 0, 'width': 0, 'height': style == 'horizontal' ? -2 : -1 }
  else
    let edges = style == 'sharp' ? ['┌', '┐', '└', '┘'] : ['╭', '╮', '╰', '╯']
    let bar = repeat('─', width / ambidouble - 2)
    let top = edges[0] .. bar .. edges[1]
    let mid = '│' .. repeat(' ', width - 2 * ambidouble) .. '│'
    let bot = edges[2] .. bar .. edges[3]
    let border = [top] + repeat([mid], height - 2) + [bot]
    let shift = { 'row': 1, 'col': 2, 'width': -4, 'height': -2 }
  endif

  let highlight = get(a:opts, 'highlight', 'Comment')
  let frame = s:create_popup(highlight, {
    \ 'row': row, 'col': col, 'width': width, 'height': height, 'border': border
  \ })
  call s:create_popup('Normal', {
    \ 'row': row + shift.row, 'col': col + shift.col, 'width': width + shift.width, 'height': height + shift.height
  \ })
  if has('nvim')
    execute 'autocmd BufWipeout <buffer> bwipeout '..frame
  endif
endfunction

let s:default_action = {
  \ 'ctrl-t': 'tab split',
  \ 'ctrl-x': 'split',
  \ 'ctrl-v': 'vsplit' }

function! s:shortpath()
  let short = fnamemodify(getcwd(), ':~:.')
  if !has('win32unix')
    let short = pathshorten(short)
  endif
  let slash = (s:is_win && !&shellslash) ? '\' : '/'
  return empty(short) ? '~'.slash : short . (short =~ escape(slash, '\').'$' ? '' : slash)
endfunction

function! s:cmd(bang, ...) abort
  let args = copy(a:000)
  let opts = { 'options': ['--multi'] }
  if len(args) && isdirectory(expand(args[-1]))
    let opts.dir = substitute(substitute(remove(args, -1), '\\\(["'']\)', '\1', 'g'), '[/\\]*$', '/', '')
    if s:is_win && !&shellslash
      let opts.dir = substitute(opts.dir, '/', '\\', 'g')
    endif
    let prompt = opts.dir
  else
    let prompt = s:shortpath()
  endif
  let prompt = strwidth(prompt) < &columns - 20 ? prompt : '> '
  call extend(opts.options, ['--prompt', prompt])
  call extend(opts.options, args)
  call skim#run(skim#wrap('SKIM', opts, a:bang))
endfunction

command! -nargs=* -complete=dir -bang SK call s:cmd(<bang>0, <f-args>)

let &cpo = s:cpo_save
unlet s:cpo_save
