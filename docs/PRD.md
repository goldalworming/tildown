# PRD — TilDown (Tilde + markdown)

Aplikasi catatan markdown yang ringan, mirip Simplenote: banyak catatan, buka instan, ketik langsung.
Dibangun dengan wgpu + winit supaya satu basis kode bisa berjalan di desktop, web dan Android.

## 1. Latar belakang

- `notex` (Tauri) lambat dibuka dan boros RAM. Penyebabnya terutama bundle web-nya: Monaco, Excalidraw + React, Mermaid dan sqlite-wasm, ditambah WebView2.
- `stylegen` dan `brosql` membuktikan stack wgpu buatan sendiri bisa ringan, dan renderer teksnya bisa dipakai ulang.
- Web dan Android menjadi target, sehingga GPUI (hanya desktop) tidak dipilih.

## 2. Tujuan

1. Membuka aplikasi dan langsung bisa mengetik dalam waktu kurang dari 0,5 detik.
2. Menangani ribuan catatan tanpa terasa lambat, baik saat mencari maupun menggulir.
3. Data milik pengguna: catatan disimpan sebagai berkas `.md` biasa.
4. Satu basis kode untuk Windows, macOS, web dan Android.

## 3. Bukan tujuan (v1)

- Excalidraw dan isi catatan berupa HTML. Flowchart Mermaid (`graph`/`flowchart`, `sequenceDiagram`) dan PlantUML (```` ```plantuml ````, boleh beberapa `@startXXX` per blok) dirender native di mode baca tanpa mermaid.js/Java/Graphviz: sequence, activity (baru & lama), class, object, use case, component/deployment/archimate, state, mindmap, WBS, gantt, timing, JSON, YAML, nwdiag, salt, EBNF, regex, Chen ER, chart, files, board, HCL, creole, DOT, flow, git, wire, ditaa. Diagram berbasis Graphviz ditata mesin berlapis sendiri (`plantuml/graph.rs`), jadi mirip tetapi tidak identik. Tidak didukung: `@startmath`/`@startlatex`, sudoku, `!include`/makro preprocessor, sprite/ikon; blok yang tidak dikenali tampil sebagai kode.
- Kolaborasi real-time.
- Sinkronisasi cloud bawaan. Folder catatan bisa disinkronkan dengan Syncthing, Dropbox, iCloud dan sejenisnya.
- Plugin dan tema buatan pengguna.
- Pratinjau WYSIWYG penuh seperti Typora.

## 4. Pengguna dan skenario

Pengguna tunggal yang mencatat cepat: ide, potongan kode, daftar tugas, catatan rapat.

- **S1.** Buka aplikasi → catatan terakhir sudah terbuka → langsung mengetik.
- **S2.** Ketik di kotak cari → daftar catatan tersaring saat mengetik → Enter membuka catatan teratas. Kalau tidak ada yang cocok, Enter membuat catatan baru dengan judul dari teks pencarian (pola Notational Velocity).
- **S3.** Baca catatan panjang dengan heading, list, kode dan gambar dalam tampilan yang rapi.
- **S4.** Di HP: buka, cari dan edit catatan yang sama (folder disinkronkan pihak ketiga).

## 5. Persyaratan non-fungsional

Patokan diukur di mesin terlemah, yaitu laptop Dell dengan Intel HD 520.

| Ukuran | Target | Terukur (2026-10-14, release, backend GL) |
|---|---|---|
| Jendela tampil | < 150 ms | 114–179 ms saat hangat; ~1,2 s pada peluncuran pertama setelah build (pemindaian antivirus) |
| RAM (Private Working Set) dengan 10.000 catatan | < 80 MB | 67 MB (10.000 catatan, 18 MB teks); 26–35 MB dengan beberapa catatan |
| Memuat 10.000 catatan | < 1 s | 622–755 ms |
| Pencarian pada 10.000 catatan | < 30 ms per ketikan | 1–3 ms |
| Relayout per ketikan, dokumen 1 MB | < 16 ms | maks. 9,1 ms; membuka dokumen 1 MB pertama kali 257 ms |
| CPU saat diam | 0% (`ControlFlow::Wait`, tidak ada animasi; cursor tidak berkedip) | — |
| Ukuran binary Windows | < 10 MB | — |
| Kehilangan data | Nol: autosave atomik (`.tmp` + fsync + rename) 1 detik setelah berhenti mengetik, paling lambat 5 detik, juga saat jendela kehilangan fokus, ditutup atau di-suspend | — |

## 6. Arsitektur

```
src/
  doc.rs      inti editor: teks + cursor + seleksi + undo (diff). Murni, tanpa platform.
  store.rs    folder .md: muat, simpan atomik, rescan perubahan dari luar, pencarian.
  md.rs       highlight per baris (editor) + pulldown-cmark → blok (mode baca).
  text.rs     font (ab_glyph), atlas glyph sisi CPU, layout + word-wrap + hit-test.
  gpu.rs      wgpu: satu pipeline quad bertekstur (teks, rect, gambar).
  app.rs      shell winit + seluruh UI: sidebar, cari, editor, mode baca.
  web.rs      khusus wasm: cermin OPFS (pengganti store::disk), clipboard, link.
web/          index.html, build.sh, font Noto; hasil build di web/pkg/.
```

**Aturan lapisan:** `doc`, `store` dan `md` tidak boleh mengimpor winit atau wgpu. Shell menerjemahkan event platform menjadi perintah (`insert`, `backspace`, `set_selection`, ...). IME Android nantinya memanggil perintah yang sama.

### Keputusan teknis

| Keputusan | Alasan |
|---|---|
| winit untuk semua platform, termasuk Windows | Satu shell untuk desktop, web dan Android. Konsekuensinya, shell Win32 buatan sendiri dari brosql tidak dipakai. |
| GL di Windows, DX12 sebagai cadangan otomatis | Diukur di HD 520 dengan aplikasi lengkap: GL 26 MB / jendela 132 ms, DX12 105 MB / 1.074 ms, tampilan identik. Teks memakai antialias grayscale, jadi dual-source blending tidak dibutuhkan. Vulkan tidak dicoba karena crash saat membuat device di HD 520 (`brosql/src/render/pipeline.rs`). `WGPU_BACKEND=dx12` memaksa DX12. |
| `MemoryHints::MemoryUsage` | Default `Performance` memesan blok heap sekitar 256 MB, dan di iGPU itu langsung menjadi RAM proses. |
| `ab_glyph` di semua platform, bukan `fontdue` | fontdue mem-parse semua outline saat font dimuat: 59 MB untuk 4 font Segoe UI/Consolas. ab_glyph membaca outline saat glyph dibutuhkan. Teks LCD subpixel (baker GDI brosql) belum ada. |
| Penyimpanan berupa folder `.md` dan indeks di memori, tanpa database | Terukur: 10.000 catatan (18 MB) dimuat 0,7 s, dicari dalam 1–3 ms. SQLite FTS tidak dibutuhkan. |
| Nama berkas = id stabil berupa stempel waktu (`20261014-090000.md`); judul diambil dari baris pertama | Mengganti judul tidak memicu rename yang membingungkan alat sinkronisasi. |
| `pulldown-cmark` untuk parser | Standar CommonMark + GFM (tabel, task list), tanpa dependensi berat. |

## 7. Fitur per milestone

Setiap milestone punya kriteria selesai yang bisa dicek.

### M0 — Skeleton ✅
- Jendela winit, device wgpu (DX12), clear frame, resize, lifecycle `resumed`/`suspended`.
- `Doc` dengan insert, backspace, delete, panah, seleksi dan select-all, beserta test.
- **Selesai bila:** `cargo test` lulus; mengetik mengubah jumlah karakter di judul jendela.
- Status: selesai.

### M1 — Renderer teks dan editor dasar ✅
- Pipeline quad + atlas glyph R8 2048² dengan kunci (font, ukuran, karakter).
- Editor dengan word-wrap, cursor, seleksi (mouse, Shift, klik ganda/tiga kali), scroll, copy/cut/paste, undo/redo (diff, ketikan beruntun digabung).
- Navigasi atas/bawah (kolom dipertahankan), PageUp/Down, Home/End per baris visual, Ctrl+panah, Ctrl+Home/End. IME desktop: `Ime::Preedit` dan `Ime::Commit`.
- **Selesai bila:** dokumen 1 MB bisa diketik tanpa lag (< 16 ms per frame) di HD 520.
- Status: terpenuhi untuk relayout (maks. 9,1 ms per ketikan, hanya baris yang berubah yang di-layout ulang). Waktu menggambar frame sendiri belum diukur terpisah, tetapi hanya baris yang terlihat yang digambar.

### M2 — Banyak catatan ✅
- Folder catatan (default `Documents/TilDown`, bisa diganti dengan `MARKDOWN_DIR`), autosave atomik, rescan saat jendela kembali aktif.
- Sidebar daftar catatan (judul, cuplikan, waktu) urut terbaru, hanya baris terlihat yang digambar.
- Kotak cari ala Notational Velocity: semua kata harus ada di teks; di bawahnya hasil fuzzy (huruf berurutan di judul, `blj` → "Belanja"). Enter membuka hasil persis teratas atau membuat catatan baru, panah atas/bawah berpindah catatan, Esc mengosongkan. Hapus memindahkan berkas ke `.trash/`. Catatan baru yang ditinggalkan kosong tidak ditulis ke disk.
- Pintasan: Ctrl+N baru, Ctrl+F/Ctrl+L cari, Ctrl+E edit/baca, Ctrl+S simpan sekarang.
- **Selesai bila:** 10.000 catatan hasil seed dimuat < 1 detik, pencarian < 30 ms, RAM < 80 MB.
- Status: terpenuhi (0,6–0,75 s; 1–3 ms; 67 MB).

### M3 — Markdown ✅
- Highlighting di editor: heading lebih besar, **bold** benar-benar tebal, `kode` monospace, penanda `#`/`*` diredupkan (gaya iA Writer). Enter melanjutkan list dan task list; Enter di item kosong mengakhiri list.
- Mode baca: heading, paragraf, list bersarang, quote, blok kode, tabel, gambar lokal (PNG/JPEG/GIF/WebP, maks. 1600 px), garis, link (hanya http/https/mailto yang dibuka), checkbox task list yang bisa diklik dan langsung mengubah sumbernya.
- **Selesai bila:** README brosql dan stylegen tampil benar di mode baca.
- Status: terpenuhi. Batasnya: karakter yang tidak ada di font sistem (emoji, `↔`, beberapa ikon) tampil sebagai kotak, karena belum ada font cadangan per karakter.

### M4 — Web ✅
- Target `wasm32`: entry `wasm-bindgen`, backend WebGPU dengan cadangan WebGL2 (`wgpu::util::is_browser_webgpu_supported` dicek dulu, karena kanvas terkunci ke konteks pertama).
- Penyimpanan: OPFS (Origin Private File System). Semua berkas dimuat ke cermin di memori saat start (`src/web.rs`); simpan = tulis cermin + antrean tulis OPFS di latar. `Store` tidak berubah, hanya celah `disk`.
- Font Noto Sans (OFL) ditanam di wasm (`web/fonts/`). Build: `sh web/build.sh`, lalu sajikan `web/` dengan server statis.
- **Selesai bila:** S1–S3 berjalan di Chrome dan Firefox.
- Status: terpenuhi, diuji headless lewat CDP (Chrome/WebGPU) dan WebDriver BiDi (Firefox/WebGL2): mengetik, autosave ke OPFS, buka ulang membuka catatan terakhir, mode baca. Batasnya:
  - Belum ada ekspor/impor `.zip`.
  - Ctrl+V hanya menempel teks yang disalin di dalam aplikasi; menyalin ke luar sudah bisa.
  - Gambar lokal tidak tampil.
  - Ctrl+N ditangkap Chrome; pakai tombol Baru atau Enter di kotak cari.
  - wasm 6 MB (2,3 MB gzip), terutama karena wgpu-core + naga untuk WebGL dan 1,7 MB font.

### M5 — Android
- `cdylib` + `android_main` (`android-activity`, GameActivity), build dengan `cargo-ndk` + Gradle.
- Keyboard layar dan IME melalui GameTextInput: composition, autocorrect, saran kata.
- Sentuhan: scroll dengan momentum, long-press untuk memilih, handle seleksi.
- Simpan saat `suspended`; surface dibuat ulang saat `resumed` (sudah ada di M0).
- **Selesai bila:** mengetik bahasa Indonesia dengan Gboard (termasuk autocorrect dan glide) terasa normal.

### M6 — Rilis
- Installer Windows (pola stylegen), bundle macOS, APK.
- Pembaruan otomatis tidak termasuk v1.

## 8. Risiko

| Risiko | Dampak | Mitigasi |
|---|---|---|
| IME Android lewat winit lemah | Tinggi: aplikasi catatan yang tidak bisa diketik dengan nyaman tidak ada gunanya | Spike di awal M5. Kalau gagal dalam 1 minggu, pakai `EditText` native untuk editor di Android saja; wgpu tetap untuk daftar dan mode baca. |
| Layout rich text (wrap gaya campuran, tabel) memakan waktu | Sedang | M3 dimulai dari highlighting di editor; mode baca penuh boleh menyusul. |
| Init GPU lambat di HD 520 | Rendah (GL: jendela ~130 ms) | Bila backend lain dipakai: jendela tampil dulu, GPU disiapkan di thread latar (pola brosql). |
| Driver GL di mesin lain rusak atau lambat | Sedang | DX12 otomatis dicoba bila GL gagal dibuat; `WGPU_BACKEND` untuk memaksa. |
| winit menambah overhead dibanding Win32 langsung | Rendah | Ukur di M1; shell Win32 bisa dikembalikan khusus untuk Windows karena `doc` terpisah dari shell. |

## 9. Pertanyaan terbuka

- Apakah catatan dari `notex` perlu diimpor? Kalau ya, dari format apa?
- Apakah tag dan folder perlu ada, atau cukup pencarian (seperti Simplenote)?
- Apakah teks LCD subpixel di Windows (gaya brosql) wajib di v1?
