#!/bin/bash
set -e

echo "Building goto..."
cargo build --release

# Install binary
INSTALL_DIR="${HOME}/.local/bin"
mkdir -p "$INSTALL_DIR"
cp target/release/goto "$INSTALL_DIR/"
echo "Installed binary to $INSTALL_DIR/goto"

# Get the shell config file
SHELL_CONFIG=""
if [[ "$SHELL" == *"zsh"* ]]; then
    SHELL_CONFIG="$HOME/.zshrc"
elif [[ "$SHELL" == *"bash"* ]]; then
    SHELL_CONFIG="$HOME/.bashrc"
fi

# Copy shell function
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GOTO_SHELL="$HOME/.config/goto/goto.zsh"
mkdir -p "$(dirname "$GOTO_SHELL")"
cp "$SCRIPT_DIR/goto.zsh" "$GOTO_SHELL"
echo "Installed shell function to $GOTO_SHELL"

# Check if already sourced in shell config
if [[ -n "$SHELL_CONFIG" ]]; then
    if grep -q "goto.zsh" "$SHELL_CONFIG" 2>/dev/null; then
        echo "Shell function already configured in $SHELL_CONFIG"
    else
        echo "" >> "$SHELL_CONFIG"
        echo "# goto - Quick project navigation" >> "$SHELL_CONFIG"
        echo "source \"$GOTO_SHELL\"" >> "$SHELL_CONFIG"
        echo "Added source line to $SHELL_CONFIG"
    fi
fi

# Ensure binary is in PATH
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    if [[ -n "$SHELL_CONFIG" ]]; then
        if ! grep -q "\.local/bin" "$SHELL_CONFIG" 2>/dev/null; then
            echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$SHELL_CONFIG"
            echo "Added $INSTALL_DIR to PATH in $SHELL_CONFIG"
        fi
    fi
fi

echo ""
echo "Installation complete!"
echo ""
echo "Next steps:"
echo "  1. Restart your terminal or run: source $SHELL_CONFIG"
echo "  2. Run: goto scan"
echo "  3. Try: goto <project-name>"
echo ""
echo "Configuration:"
echo "  - Config file: ~/Library/Application Support/dev.goto.goto/config.toml"
echo "  - To add scan paths: goto add ~/projects"
