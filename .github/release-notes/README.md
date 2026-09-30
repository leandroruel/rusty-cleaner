# Release notes

Every release tag gets its curated changelog from this folder. To ship a
release with a detailed summary:

1. Write `.github/release-notes/v<version>.md` and commit it — **before**
   pushing the tag, because the release workflow checks out the tag itself.
2. Push the tag. The workflow uses the file as the release body and appends
   the automatically generated commit list below it.

If no file matches the tag, the release falls back to the auto-generated
notes only.

Style: English, GitHub-flavored markdown, sections per feature area, the
"why" before the "what", concrete numbers when available, and the
`**Full Changelog**` link at the bottom.
