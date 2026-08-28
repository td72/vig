Output assets/demo-docker.gif

Require vig
Require docker

Set Shell "bash"
Set FontFamily "Hack Nerd Font Mono"
Set FontSize 16
Set Width 1200
Set Height 700
Set Theme "Catppuccin Frappe"
Set WindowBar Colorful
Set CursorBlink false
Set TypingSpeed 50ms

# --- Hidden setup: throwaway demo containers on the local daemon ---
Hide

Type@1ms "cd $(mktemp -d) && git init -q"
Enter
Sleep 300ms

# The recording runs against the real docker CLI, but a PATH shim hides
# containers and images that are not part of the demo (whatever else happens
# to run on the recording machine stays out of the GIF). Everything else is
# forwarded untouched, so inspect / logs / version are the real thing.
Type@1ms "mkdir -p shim && cat > shim/docker << 'EOF'"
Enter
Type@1ms "#!/bin/sh"
Enter
Type@1ms "real=$(PATH=${PATH#*:} command -v docker)"
Enter
Type@1ms 'case "$1" in'
Enter
Type@1ms `  ps) "$real" "$@" | grep -F vig-demo || true ;;`
Enter
Type@1ms `  images) "$real" "$@" | grep -E '"Repository":"(nginx|alpine)"' || true ;;`
Enter
Type@1ms `  *) exec "$real" "$@" ;;`
Enter
Type@1ms "esac"
Enter
Type@1ms "EOF"
Enter
Type@1ms "chmod +x shim/docker && export PATH=$PWD/shim:$PATH"
Enter

Type@1ms "docker rm -f vig-demo-nginx vig-demo-ticker-1 vig-demo-worker-1 vig-demo-migrate-1 >/dev/null 2>&1"
Enter
Type@1ms "docker run -d --rm --name vig-demo-nginx -p 127.0.0.1:18080:80 nginx:alpine >/dev/null"
Enter

Type@1ms "cat > compose.yaml << 'EOF'"
Enter
Type@1ms "services:"
Enter
Type@1ms "  ticker:"
Enter
Type@1ms "    image: nginx:alpine"
Enter
Type@1ms `    command: sh -c 'i=0; while true; do i=$$((i+1)); echo "tick $$i - processed batch $$i ($$((i*3)) jobs)"; sleep 1; done'`
Enter
Type@1ms "  worker:"
Enter
Type@1ms "    image: nginx:alpine"
Enter
Type@1ms "    command: sleep 600"
Enter
Type@1ms "  migrate:"
Enter
Type@1ms "    image: nginx:alpine"
Enter
Type@1ms `    command: sh -c 'echo "applying 3 migrations"; echo "done"'`
Enter
Type@1ms "EOF"
Enter
Type@1ms "docker compose -p vig-demo -f compose.yaml up -d >/dev/null 2>&1"
Enter
Sleep 4s

Type@1ms "clear"
Enter
Sleep 300ms

Show

# --- Demo: the Docker view ---

Type "vig"
Enter
Sleep 1.5s

# Switch to the Docker view: containers grouped by compose project
Type "4"
Sleep 3s

# Move down: the detail (inspect summary) and logs follow the selection
Type "j"
Sleep 2s
Type "j"
Sleep 3s

# Focus the logs: new lines arrive while following
Type "l"
Sleep 3s
# Scrolling up pauses following; G jumps to the end and resumes
Type "k"
Sleep 800ms
Type "k"
Sleep 2s
Type "G"
Sleep 2s

# Search the log
Type "/batch 3"
Enter
Sleep 2s
Escape
Sleep 800ms

# Back to the containers
Type "h"
Sleep 1s
Type "j"
Sleep 1.5s
Type "j"
Sleep 2s

# Images: the detail shows the image's inspect summary
Tab
Sleep 2.5s

# Scroll the detail pane
Tab
Sleep 1s
Type "j"
Sleep 500ms
Type "j"
Sleep 500ms
Type "j"
Sleep 1.5s
Escape
Sleep 1s

# Quit
Type "q"
Sleep 500ms

# --- Hidden teardown (the demo processes ignore SIGTERM, so stop fast) ---
Hide
Type@1ms "docker rm -f vig-demo-nginx >/dev/null 2>&1; docker compose -p vig-demo -f compose.yaml down -t 1 >/dev/null 2>&1"
Enter
Sleep 8s
