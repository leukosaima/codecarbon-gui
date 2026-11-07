# Disket Mono (optional, not in the repository)

The CodeCarbon design system uses **Disket Mono** by Mariano Diez (Rostype)
as its "Data" font for numbers. Its EULA (*Mariano Diez Free Font EULA*,
2019-09-30) allows personal and commercial use but, in clause (d), forbids
distributing the font or making it available to third parties. So the
font files are **not** committed and **not** shipped in the installers.

For a local build that uses it, put these files in this folder (they are
git-ignored):

- `DisketMono-Regular.ttf`
- `DisketMono-Bold.ttf`
- `Disket-mono_EULA.pdf` (keep the licence next to the font)

Without them the app uses an installed copy of Disket Mono if there is one,
else IBM Plex Mono Medium. If written permission from Mariano Diez to embed
the font in this open-source app is obtained, record it here and remove the
`.gitignore` rule.
