# Niko Shell Integration
# Add this to your ~/.zshrc or ~/.bashrc

# For Zsh: command appears at your prompt ready to edit/run
if [ -n "$ZSH_VERSION" ]; then
    n() {
        local cmd
        cmd=$(niko "$@" 2>/dev/null)
        if [ -n "$cmd" ]; then
            print -z "$cmd"
        fi
    }
fi

# For Bash: command appears at your prompt ready to edit/run
if [ -n "$BASH_VERSION" ]; then
    n() {
        local cmd
        cmd=$(niko "$@" 2>/dev/null)
        if [ -n "$cmd" ]; then
            # Add to readline buffer
            READLINE_LINE="$cmd"
            READLINE_POINT=${#cmd}
        fi
    }

    # Alternative: use history (works in all bash versions)
    nn() {
        local cmd
        cmd=$(niko "$@" 2>/dev/null)
        if [ -n "$cmd" ]; then
            history -s "$cmd"
            echo "$cmd"
            echo "Command added to history. Press Up arrow to access it."
        fi
    }
fi
