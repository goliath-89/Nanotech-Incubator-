# Your backlog

Things only you can do, in the order they unblock work. Tick them off as you
go; Claude adds to this list when something new needs a person.

## Do soon (unblocks Day 3 and Gate A)

- [ ] Download the `strata-cli-macos-latest` and `strata-cli-windows-latest`
      builds from the latest run under the repo's **Actions** tab on GitHub.
- [ ] On the Mac: `chmod +x strata && ./strata scan / --errors`. Save the output.
      If macOS blocks the file, run `xattr -d com.apple.quarantine strata` first.
- [ ] On the PC: unzip the download, open PowerShell **as administrator**, then:
      `cd $HOME\Downloads\strata-cli-windows-latest` (wherever you unzipped it),
      `.\strata.exe scan C:\ --errors` and `.\strata.exe scan C: --fast --errors`.
      PowerShell only runs a program from the current folder when you type `.\` in front.
- [ ] Note the used space Finder (Get Info on Macintosh HD) and Explorer
      (Properties on C:) show for those drives.
- [ ] Paste both outputs and both used-space figures into a session and say "Day 3".

## Do this week

- [ ] On the PC, in a terminal opened with "Run as administrator": `strata.exe scan C: --fast --errors`.
      Paste the output next to the normal scan's. This is the first run of the fast scan on real Windows.
- [ ] Run the 2-million-file benchmark on both machines (about 10 minutes each, needs about 5 GB free)
      and paste the two `RESULT` lines:
      `strata make-tree bench-tree 2000000`, then `strata bench bench-tree`, then delete `bench-tree`.
      On Windows run it somewhere under your user folder, with `strata.exe`.
- [ ] Tell Claude how long you expect to have Fable, so the remaining
      Fable-first work can be ordered around it.

## Decisions coming up

- [ ] Final product name: check the domain and do a trademark search for "Strata" (Day 20).
- [ ] One-time price or subscription (Day 67).
- [ ] Telemetry provider, or none (Day 69).

## Accounts and purchases (not needed before week 14)

- [ ] Apple Developer Program, $99 per year (Day 74).
- [ ] Windows code signing (Day 74).
- [ ] Paddle or Lemon Squeezy seller account (Day 67).
- [ ] Domain for the website (Day 68).
