# Writing Style

How to write anything a person will read in this project: the README, the
guidebook pages under `docs/`, copy inside the app, commit messages, and pull
request descriptions.

The voice here is not invented. It is taken from documentation the maintainer
has written himself, and from corrections he has made to text written for this
repo. Where a rule quotes something, the quote is real.

## The short version

Write like a person explaining the thing to another person who is capable but
new to it. Short declarative sentences, contractions, a reason attached to every
instruction, concrete values instead of vague ones, and no hype.

If a page reads like it was assembled from a template, it is wrong, no matter
how accurate it is.

## Sentences and paragraphs

One idea per sentence. Break a compound sentence in two rather than joining
clauses with a dash or a semicolon.

Use contractions. "Can't", "isn't", "you'll", "we're". Formal register is not
the goal.

Keep paragraphs to one to four sentences.

Write one sentence per line in the Markdown source where it helps. It keeps
diffs readable, and the rendered output is unaffected.

A one-word sentence is a legitimate answer when the question was a real one:

> But do we actually have 24 graphics EPROMs?
>
> No.

## Move by question and answer

Say the question the reader is holding, then answer it. Do not build up to the
answer, and do not make the reader infer that a question was being addressed at
all.

> The graphics and game code are stored on EPROMs, but which ones?

> This list is a subset of the files in the Gauntlet romset, and shows what the
> EPROM is used for. But how do we know this information? To answer that, let's
> look at the Gauntlet Schematic.

## Give the reason, in the same sentence

Never write a bare restriction or a bare instruction. The reader who knows why
can adapt when the situation differs; the reader who does not is stuck.

> ...standard console sprite extraction tools ... can't parse the data
> correctly, because the graphics format doesn't match what these tools expect.

This is why "Do not X." is banned as a form. Write "X doesn't work here, because
Y" instead.

## Concrete values, never vague ones

Versions, dates, counts, sizes, paths, command names. Write `MAME version 0.191,
released Oct 24, 2017`, not "a recent version". Write "three tests fail:
`test_auth`, `test_retry`, `test_paging`", not "several tests are failing".

## Point at the primary source, with its exact location

Link the vendor's own documentation, the schematic, the source file, the ADR.
Give the page number, the line, or a commit-pinned URL — not just the project
homepage.

> Pages 5-21 through 5-24 of the PCB Part List in the Gauntlet Arcade Manual
> list the following EPROMs

Read the primary source before writing a paragraph of reasoning about it. One
fetch usually settles what a paragraph of argument only guesses at.

## Correct the likely misreading before it happens

When a name, a suffix, or a term will be read wrongly, say so at the point of
first use, in one short aside.

> The file suffix is not a file extenstion and rather corresponds to the
> physical location of the chip on the PCB board.

> \* A MAME "ROM" is actually a romset, which is a compressed archive containing
> multiple files representing different chips.

## Say what isn't known, then move on

State the uncertainty once, plainly, and keep going. Don't hedge every
neighbouring sentence to cover it.

> It isn't obvious how to determine the different sections, but for now, note
> that Gauntlet has hundreds (not thousands) of colors associated with sprite
> tiles.

Once the evidence is in, commit to the conclusion. No softening:

> These are definitely our graphics chips.

## Person

- **"You"** for what the reader does. "To start, you'll need a Gauntlet romset."
- **"We"** for walking through evidence together. "Scrolling through the
  palettes, we see 3 distinct sections."
- **"I"** only for a personal recommendation. "I recommend any graphics
  tutorials for the NES."

## Give the impatient reader an exit

Put the result at the top and route anyone who does not want the explanation.

> *If you just want the sprite sheets, head over to the releases.*

Repeat the exit where the deep detail starts, not only in the introduction.

## Lower the barrier when something looks hard

Name the difficulty and then say why it does not have to be understood in full.

> *Note:* The `gauntlet.cpp` code can be intimidating at first because it
> emulates hardware. Luckily, we don't need to understand all the details, and
> we're only interested in a few data structures.

## Explain inside the example

Comment the code sample heavily and say that you did. A commented example beats
a paragraph describing what an uncommented example would have shown.

## Prose first, bullets to summarize

Explain in sentences. Then compress under a signpost — "At a high level, that
means:", "In practical terms," — if a list genuinely helps. A list that carries
the whole argument on its own is a sign the prose was never written.

Use real headings for structure. A bolded paragraph opener is not a heading, and
it breaks the table of contents.

## Product copy

The rules above apply, plus one that is specific to the app.

**Copy states what a person can do. It does not warn, alarm, or hedge.** Before
adding a caution to a screen, check whether it protects the reader or protects
the writer. If a plain statement of the capability serves just as well, write
that.

A real correction from this repo:

| | |
|---|---|
| Rejected | "Once uploading starts, removing these messages again means deleting them from the vault by hand." |
| Shipped | "Imported conversations can later be removed from your vault in the messages area." |

Both sentences describe the same situation. The first frames it as a cost and
leans on "deleting" and "by hand" to make it sound laborious. The second says
where the capability lives.

Write mockup copy for the product being built, not for the product as it stands
today. If a screen assumes a capability that is planned but unbuilt, write the
copy for the finished product and say plainly which capability it assumes,
rather than contorting the words to survive a literal reading of today's code.

### Fixed product vocabulary

These names are settled. Use them in the UI, in docs, in code, and in
conversation.

| Use | Never |
|---|---|
| **Contact Groups** — collections of contacts, usable in searches | "Groups" |
| **Saved Searches** — stored queries that re-resolve as messages arrive | "Saved Groups" |
| **Message Tags** — marks on conversations | "Thread Tags" |
| **Text Message** — the reader-facing label for iMessage and SMS/MMS alike | "iMessage", "SMS" as a UI label |

"WhatsApp" keeps its own name; the Text Message collapsing covers the Apple and
carrier texting transports only. The underlying `service` value still records
`imessage`, `sms`, and `mms` — only the presentation is collapsed.

A stale name met in passing is something to fix, not something to match.

## Commit messages and pull requests

Plain, direct English. Say what changed and why. Skip the ceremony.

A pull request description opens with what a reviewer can verify — the change
itself — and explains afterwards. When reporting finished work, lead with the
commit SHA, the PR number and its state, the diffstat as `N files changed,
+X/-Y`, and the names of tests that pass. Because this repo squash-merges, the
`(#123)` suffix on a subject line is the proof a change arrived through a pull
request; the branch itself will be gone.

Do not close a report with a reminder that a known problem is still unfixed, or
with an unprompted offer to fix it sooner. State a deferred item once, where it
is found or where the plan defers it, then leave it in the issue.

## What not to write

- Marketing hooks and rhetorical openers. A plain functional lead beats "So why
  don't you own them?"
- Template boilerplate. "Contributions are greatly appreciated" says nothing.
- Clipped imperatives with no reason attached.
- Warnings that dramatize consequences.
- Design-pattern vocabulary in reader-facing text — seam, deep module, adapter,
  leverage, locality — and library terms used before the thing they name has
  been shown.
- Routine, documented practice written up as a deviation or a risk.
- Invented names. Never coin a term for something that already has one, and
  never make up a filename, endpoint, or config key you have not read.

## Checking the result

Markdown source is not the deliverable; the rendered page is. After editing
anything under `docs/`, run the build and look at what comes out — heading
hierarchy, table of contents, tables, checkboxes:

```bash
cd docs && npm run check && npm run build
```
