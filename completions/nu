module completions {

  def "nu-complete watchexec screen_clear" [] {
    [ "clear" "reset" ]
  }

  def "nu-complete watchexec on_busy_update" [] {
    [ "queue" "do-nothing" "restart" "signal" ]
  }

  def "nu-complete watchexec emit_events_to" [] {
    [ "environment" "stdio" "file" "json-stdio" "json-file" "none" ]
  }

  def "nu-complete watchexec color" [] {
    [ "auto" "always" "never" ]
  }

  def "nu-complete watchexec filter_fs_events" [] {
    [ "access" "create" "remove" "rename" "modify" "metadata" ]
  }

  def "nu-complete watchexec completions" [] {
    [ "bash" "elvish" "fish" "nu" "powershell" "zsh" ]
  }

  # Execute commands when watched files change
  export extern watchexec [
    ...command: string        # Command to run on changes
    --watch(-w): string       # Watch a specific file or directory
    --clear(-c): string@"nu-complete watchexec screen_clear" # Clear screen before running command
    --on-busy-update(-o): string@"nu-complete watchexec on_busy_update" # What to do when receiving events while the command is running
    --watch-when-idle(-W)     # Deprecated alias for '--on-busy-update=do-nothing'
    --restart(-r)             # Restart the process if it's still running
    --signal(-s): string      # Send a signal to the process when it's still running
    --kill(-k)                # Hidden legacy shorthand for '--signal=kill'
    --stop-signal: string     # Signal to send to stop the command
    --stop-timeout: string    # Time to wait for the command to exit gracefully
    --map-signal: string      # Translate signals from the OS to signals to send to the command
    --debounce(-d): string    # Time to wait for new events before taking action
    --stdin-quit              # Exit when stdin closes
    --no-vcs-ignore           # Don't load gitignores
    --no-project-ignore       # Don't load project-local ignores
    --no-global-ignore        # Don't load global ignores
    --no-default-ignore       # Don't use internal default ignores
    --no-discover-ignore      # Don't discover ignore files at all
    --ignore-nothing          # Don't ignore anything at all
    --postpone(-p)            # Wait until first change before running command
    --delay-run: string       # Sleep before running the command
    --poll: string            # Poll for filesystem changes
    --shell: string           # Use a different shell
    -n                        # Don't use a shell
    --no-shell-long           # Don't use a shell
    --no-environment          # Shorthand for '--emit-events=none'
    --emit-events-to: string@"nu-complete watchexec emit_events_to" # Configure event emission
    --only-emit-events        # Only emit events to stdout, run no commands
    --env(-E): string         # Add env vars to the command
    --no-process-group        # Don't use a process group
    -1                        # Testing only: exit Watchexec after the first run
    --notify(-N)              # Alert when commands start and end
    --color: string@"nu-complete watchexec color" # When to use terminal colours
    --timings                 # Print how long the command took to run
    --quiet(-q)               # Don't print starting and stopping messages
    --bell                    # Ring the terminal bell on command completion
    --project-origin: string  # Set the project origin
    --workdir: string         # Set the working directory
    --exts(-e): string        # Filename extensions to filter to
    --filter(-f): string      # Filename patterns to filter to
    --filter-file: string     # Files to load filters from
    --ignore(-i): string      # Filename patterns to filter out
    --ignore-file: string     # Files to load ignores from
    --fs-events: string@"nu-complete watchexec filter_fs_events" # Filesystem events to filter to
    --no-meta                 # Don't emit fs events for metadata changes
    --print-events            # Print events that trigger actions
    --verbose(-v)             # Set diagnostic log level
    --log-file: string        # Write diagnostic logs to a file
    --manual                  # Show the manual page
    --completions: string@"nu-complete watchexec completions" # Generate a shell completions script
    --help(-h)                # Print help (see more with '--help')
    --version(-V)             # Print version
  ]

}

export use completions *
