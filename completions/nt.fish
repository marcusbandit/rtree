complete -c rtree -s L -l level -d 'Descend only N directories deep' -r
complete -c rtree -s P -l pattern -d 'List only files/dirs whose name contains PATTERN (case-insensitive)' -r
complete -c rtree -s I -l ignore -d 'Do NOT list files/dirs whose name contains PATTERN (case-insensitive)' -r
complete -c rtree -l color -d 'Color mode: always, auto (default), simple, never' -r -f -a "always\t'Full file-type coloring'
auto\t'Smart: no color when piped, simple when output is busy, full otherwise'
simple\t'Color directories and symlinks only'
never\t'No color'"
complete -c rtree -l search -d 'Pre-fill the TUI search box' -r
complete -c rtree -l generate-completions -d 'Generate shell completions and print to stdout' -r -f -a "bash\t''
elvish\t''
fish\t''
powershell\t''
zsh\t''"
complete -c rtree -s a -l all -d 'All files are listed (including hidden)'
complete -c rtree -s d -l dirs-only -d 'List directories only'
complete -c rtree -s l -l follow-links -d 'Follow symbolic links like directories'
complete -c rtree -s f -l full-path -d 'Print the full path prefix for each file'
complete -c rtree -s x -l one-file-system -d 'Stay on the current filesystem only'
complete -c rtree -l prune -d 'Prune empty directories from the output'
complete -c rtree -s v -l version-sort -d 'Sort files alphanumerically by version (natural sort)'
complete -c rtree -s t -l time-sort -d 'Sort files by last modification time'
complete -c rtree -s c -l change-sort -d 'Sort files by last status change time'
complete -c rtree -s U -l unsorted -d 'Leave files unsorted'
complete -c rtree -s r -l reverse -d 'Reverse the order of the sort'
complete -c rtree -s s -l size -d 'Print file sizes; directories show recursive total'
complete -c rtree -s H -l human -d 'Human-readable sizes (implies -s)'
complete -c rtree -s p -l permissions -d 'Print file type and permissions, e.g. [drwxr-xr-x]'
complete -c rtree -s D -l date -d 'Print the date of last modification (or status change with -c)'
complete -c rtree -s J -l json -d 'Print a JSON representation of the tree'
complete -c rtree -s X -l xml -d 'Print an XML representation of the tree'
complete -c rtree -l tui -d 'Launch the interactive TUI instead of streaming to stdout'
complete -c rtree -s h -l help -d 'Print help (see more with \'--help\')'
