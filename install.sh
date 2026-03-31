#!/bin/bash
set -e

REPO="sderosiaux/goto"
INSTALL_DIR="${HOME}/.local/bin"
GOTO_SHELL="$HOME/.config/goto/goto.zsh"

# ── Binary ───────────────────────────────────────────────────────────────────

install_binary() {
    mkdir -p "$INSTALL_DIR"

    # Build from source only when running inside the goto repo
    if [ -f "Cargo.toml" ] && [ -d "src" ] && command -v cargo &>/dev/null; then
        echo "Building goto from source..."
        cargo build --release
        cp target/release/goto "$INSTALL_DIR/goto"
        echo "Installed binary to $INSTALL_DIR/goto"
    else
        echo "Downloading latest goto release..."
        LATEST=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
            | grep '"tag_name"' | sed 's/.*"tag_name": *"\(.*\)".*/\1/')

        if [ -z "$LATEST" ]; then
            echo "Error: could not fetch latest release. Install Rust and retry." >&2
            exit 1
        fi

        TMP=$(mktemp -d)
        curl -fsSL "https://github.com/${REPO}/releases/download/${LATEST}/goto-macos.tar.gz" \
            -o "$TMP/goto-macos.tar.gz"
        tar xzf "$TMP/goto-macos.tar.gz" -C "$TMP"
        cp "$TMP/goto" "$INSTALL_DIR/goto"
        chmod +x "$INSTALL_DIR/goto"
        rm -rf "$TMP"
        echo "Installed goto ${LATEST} to $INSTALL_DIR/goto"
    fi
}

# ── Shell function ────────────────────────────────────────────────────────────

install_shell_function() {
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    mkdir -p "$(dirname "$GOTO_SHELL")"

    if [ -f "$SCRIPT_DIR/goto.zsh" ]; then
        cp "$SCRIPT_DIR/goto.zsh" "$GOTO_SHELL"
    else
        curl -fsSL "https://raw.githubusercontent.com/${REPO}/main/goto.zsh" -o "$GOTO_SHELL"
    fi
    echo "Installed shell function to $GOTO_SHELL"
}

# ── Shell config ──────────────────────────────────────────────────────────────

configure_shell() {
    SHELL_CONFIG=""
    if [[ "$SHELL" == *"zsh"* ]]; then
        SHELL_CONFIG="$HOME/.zshrc"
    elif [[ "$SHELL" == *"bash"* ]]; then
        SHELL_CONFIG="$HOME/.bashrc"
    fi

    if [[ -z "$SHELL_CONFIG" ]]; then
        return
    fi

    if grep -q "goto.zsh" "$SHELL_CONFIG" 2>/dev/null; then
        echo "Shell function already configured in $SHELL_CONFIG"
    else
        echo "" >> "$SHELL_CONFIG"
        echo "# goto - Quick project navigation" >> "$SHELL_CONFIG"
        echo "source \"$GOTO_SHELL\"" >> "$SHELL_CONFIG"
        echo "Added source line to $SHELL_CONFIG"
    fi

    if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
        if ! grep -q "\.local/bin" "$SHELL_CONFIG" 2>/dev/null; then
            echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$SHELL_CONFIG"
            echo "Added $INSTALL_DIR to PATH in $SHELL_CONFIG"
        fi
    fi
}

# ── Pre-commit hook (dev only) ────────────────────────────────────────────────

install_hook() {
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    if [ -d "$SCRIPT_DIR/.git" ] && [ -f "$SCRIPT_DIR/hooks/pre-commit" ]; then
        cp "$SCRIPT_DIR/hooks/pre-commit" "$SCRIPT_DIR/.git/hooks/pre-commit"
        chmod +x "$SCRIPT_DIR/.git/hooks/pre-commit"
        echo "Installed pre-commit hook (fmt + clippy)"
    fi
}

# ── Main ──────────────────────────────────────────────────────────────────────

install_binary
install_shell_function
configure_shell
install_hook

echo ""
echo "Installation complete!"
echo ""
echo "Next steps:"
echo "  1. Restart your terminal or run: source ~/.zshrc"
echo "  2. Run: goto add ~/code"
echo "  3. Run: goto update       (downloads ~80MB model on first run)"
echo "  4. Try: goto <project>"
echo ""
echo "Config: ~/Library/Application Support/dev.goto.goto/config.toml"
