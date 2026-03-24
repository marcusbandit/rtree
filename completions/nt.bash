_rtree() {
    local i cur prev opts cmd
    COMPREPLY=()
    if [[ "${BASH_VERSINFO[0]}" -ge 4 ]]; then
        cur="$2"
    else
        cur="${COMP_WORDS[COMP_CWORD]}"
    fi
    prev="$3"
    cmd=""
    opts=""

    for i in "${COMP_WORDS[@]:0:COMP_CWORD}"
    do
        case "${cmd},${i}" in
            ",$1")
                cmd="rtree"
                ;;
            *)
                ;;
        esac
    done

    case "${cmd}" in
        rtree)
            opts="-a -d -l -f -x -L -P -I -v -t -c -U -r -s -H -p -D -J -X -h --all --dirs-only --follow-links --full-path --one-file-system --level --pattern --ignore --prune --version-sort --time-sort --change-sort --unsorted --reverse --size --human --permissions --date --color --json --xml --tui --search --generate-completions --help [PATH]"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 1 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --level)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -L)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --pattern)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -P)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --ignore)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -I)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --color)
                    COMPREPLY=($(compgen -W "always auto simple never" -- "${cur}"))
                    return 0
                    ;;
                --search)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --generate-completions)
                    COMPREPLY=($(compgen -W "bash elvish fish powershell zsh" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
    esac
}

if [[ "${BASH_VERSINFO[0]}" -eq 4 && "${BASH_VERSINFO[1]}" -ge 4 || "${BASH_VERSINFO[0]}" -gt 4 ]]; then
    complete -F _rtree -o nosort -o bashdefault -o default rtree
else
    complete -F _rtree -o bashdefault -o default rtree
fi
