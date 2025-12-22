# goto - Quick project navigation with fuzzy matching
# Add this to your ~/.zshrc: source /path/to/goto.zsh

# Main goto function
goto() {
    if [[ -z "$1" ]]; then
        command goto --help
        return 1
    fi

    # Commands that don't need cd
    case "$1" in
        scan|list|config|add|remove|refresh|--help|-h|--version|-V)
            command goto "$@"
            return $?
            ;;
        find)
            if [[ "$2" == "-a" || "$2" == "--all" ]]; then
                command goto "$@"
                return $?
            fi
            ;;
    esac

    # Single invocation with both streams captured via temp file
    local tmpfile=$(mktemp)
    local output

    output=$(command goto "$@" 2>"$tmpfile")
    local exit_code=$?
    local stderr_output=$(cat "$tmpfile")
    rm -f "$tmpfile"

    if [[ $exit_code -ne 0 ]]; then
        echo "$stderr_output" >&2
        return $exit_code
    fi

    # Validate: must be single line, absolute path, and directory
    if [[ -n "$output" && "$output" == /* && -d "$output" && $(echo "$output" | wc -l) -eq 1 ]]; then
        echo "\033[32m→\033[0m $output"
        cd "$output" || return 1

        # Extract post command from stderr (safer parsing with grep -F)
        local post_cmd=$(echo "$stderr_output" | grep -F "__GOTO_POST_CMD__:" | sed 's/^__GOTO_POST_CMD__://')

        if [[ -n "$post_cmd" ]]; then
            # Whitelist-based execution (NO EVAL for security)
            case "$post_cmd" in
                claude|code|cursor|vim|nvim|emacs|hx|zed)
                    echo "\033[90mRunning: $post_cmd\033[0m"
                    command "$post_cmd"
                    ;;
                *)
                    echo "\033[33mWarning:\033[0m post_command '$post_cmd' not in whitelist (claude, code, cursor, vim, nvim, emacs, hx, zed)" >&2
                    ;;
            esac
        fi
    else
        echo "$stderr_output" >&2
        return 1
    fi
}

# Completion function
_goto_completions() {
    local curcontext="$curcontext" state line
    typeset -A opt_args

    _arguments -C \
        '1: :->command' \
        '*: :->args'

    case $state in
        command)
            local commands=(
                'scan:Scan and index projects'
                'list:List indexed projects'
                'config:Show configuration'
                'add:Add a path to scan'
                'remove:Remove a path from scan'
                'refresh:Clear cache and re-scan'
                'find:Find a project by query'
            )
            _describe 'command' commands
            ;;
        args)
            case $line[1] in
                add|remove)
                    _files -/
                    ;;
                list)
                    local sorts=('recent' 'frecency' 'name')
                    _describe 'sort order' sorts
                    ;;
            esac
            ;;
    esac
}

compdef _goto_completions goto
