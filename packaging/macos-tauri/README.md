# PdfCraft Desktop for Apple Silicon

Download the `.dmg` from a successful **PdfCraft desktop for Apple Silicon** run in GitHub Actions.
Open the disk image, drag **PdfCraft Desktop** to **Applications**, and launch that copy. The `.app.zip`
artifact contains the same app bundle. These builds use an ad-hoc signature and require approval
in macOS Privacy & Security if Gatekeeper blocks a downloaded copy.

- Open multiple PDFs with **File → Open**, Finder's **Open With → PdfCraft Desktop**, or by dropping them
  into the editor. Each document has its own tab.
- **⌘S** saves to the open PDF. **⇧⌘S** opens the native Save As dialog. An untitled document uses
  the save dialog on its first save.
- PdfCraft runs the editor and OCR locally. The bundle includes the OCR models and the fonts and
  translations embedded by the build.

## Build

Run on an Apple Silicon Mac with Xcode command-line tools and Rust 1.96. The workflow checks out
the pinned upstream font and translation inputs. For an equivalent local build, point
`CRAFT_FONTS_DIR` and `CRAFT_I18N_DIR` at absolute paths to those checkouts, then run:

```sh
CRAFT_I18N_REQUIRED=1 bash packaging/macos-tauri/package.sh
```

The build downloads and verifies the OCR models using PdfCraft's existing packaging helper. It
writes the disk image and zipped app bundle to `dist/release`.

## App layout

The outer bundle runs Tauri 2 as a background launcher. It receives macOS document and reopen
events and forwards them through Launch Services to the nested native PdfCraft editor. The editor
owns the window, the original egui interface and PDF engine, the native menu, and document I/O.
This keeps the full desktop feature set, including native print, OCR and recovery, available in the
same app. The separate bundle uses its own preferences and recovery folder.
