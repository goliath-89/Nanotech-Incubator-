# Strata usability test (Day 38, for Gate B)

**Goal:** check that people who have never seen Strata can understand it
without help. **Gate B passes** when testers find the largest folder on
their drive in **3 clicks or fewer**, on both Windows and macOS.

- 5 people, about 15 minutes each, on **their own computer** (their own
  files make the test real). Mix of comfortable and less technical people.
- You run it in person or on a video call with screen sharing.
- Windows first; macOS testers once the Mac build is ready (Day 16).

## Before the session (5 minutes, you)

1. Download the newest `strata-app-windows-x64` (or `-arm64` for Snapdragon
   laptops) from the repository's **Actions** tab and send the tester the
   `.exe` installer (a shared drive or a message attachment is fine).
2. Tell them: "Windows will warn that the app is not recognised, because it
   is not signed yet. Click **More info**, then **Run anyway**." (Signing
   comes on Day 75.)
3. Have this page and the results sheet (below) open.

## What to say at the start (read it out)

> "Thank you for helping. We are testing the app, not you; there are no
> wrong answers. Please think aloud: say what you are looking at and what
> you expect to happen. I will not help, because I want to see where the
> app is unclear. Strata only reads your files; nothing leaves your
> computer, and it never deletes anything without asking. Is it OK if I
> take notes?"

Do not show them the app first. Do not point or hint. If they are stuck
for more than 60 seconds, note it, then say "Let's move on".

## Tasks

Count every click (and key press that moves somewhere) from the start of
the task. Write the number and anything they say.

| # | Say to the tester | Success | Note |
| --- | --- | --- | --- |
| 1 | "Open Strata and find out what is using space on your main drive." | They start a scan (and, on Windows, understand the administrator prompt) | Did the prompt worry them? |
| 2 | **"Which folder on your drive takes the most space?"** | They name it, or click into it | **Clicks from the finished scan: this is Gate B (≤ 3).** |
| 3 | "How much of your drive is videos?" | They read it from the By kind bar | Clicks; did they use the ring, the list or the kinds? |
| 4 | "Which app takes the most space on this computer?" | Apps tab, first app | Clicks |
| 5 | "Find the single largest file and show it in Explorer." | Largest Files → Show in Explorer | Clicks |
| 6 | "If you wanted to free up some space safely, where would you look?" | Clean Up tab; they understand nothing is deleted without a review | Ask: "What do you think happens if you click Move to Recycle Bin?" |
| 7 | "Go back to the top of the drive." | Breadcrumb, ring centre or Backspace | Clicks |

## Afterwards (3 minutes)

Ask, and note the answers in their words:

1. "What was confusing or surprising?"
2. "Was anything slow?"
3. "On a scale of 1 to 5, how easy was it to use?" (1 = very hard, 5 = very easy)
4. "Would you use this again? Why or why not?"

## Results sheet

Copy this table into a `.txt` file and send it to me after the sessions.

| Tester | System | Task 2 clicks | Tasks done (of 7) | Ease 1–5 | Biggest confusion |
| --- | --- | --- | --- | --- | --- |
| 1 | Windows 11 | | | | |
| 2 | | | | | |
| 3 | | | | | |
| 4 | | | | | |
| 5 | | | | | |

Gate B: every tester's **Task 2 clicks ≤ 3**. Day 39 fixes whatever the
sessions show; Day 40 checks the gate.

## Notes for you

- Ending early is fine if someone is uncomfortable.
- Do not install on a computer that is not theirs (work laptops may block
  unsigned apps).
- Delete your notes' personal details (names of their folders and files)
  before sending them to me; "a large video folder" is enough.
