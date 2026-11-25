# Fastplorer

**Fastplorer, Windows için hızlı bir dosya gezgini ve dosya arama aracıdır.** Bir kısayola (varsayılan **F9**) basarsınız, ortada küçük bir pencere açılır. Yazmaya başladığınız anda sonuçlar gelir. Dosyayı açar, klasörün içine girer, kopyalar, taşır, adını değiştirir ya da Geri Dönüşüm Kutusu'ndan geri yüklersiniz. Bunların hepsi klavyeyle yapılabilir; isterseniz yalnız fareyle de kullanılabilir.

*English summary below.*

## Neden hızlı?

- **Hazır dizin:** Sabit sürücüler arka planda paralel taranır ve dosya adları sıkıştırılmış bir dizine yazılır. Dizin diske kaydedilir, sonraki açılışta hemen yüklenir. Tarama sürerken de arama yapılabilir. Sistem klasörleri (Windows, `$Recycle.Bin` vb.) atlanır.
- **Yazarken arama:** Her tuş vuruşunda dizinde çok iş parçacıklı arama yapılır. Sonuçlar puanlanarak sıralanır: önek, kelime başı, içerir, sonra bulanık eşleşme. Türkçe harfler katlanır (ı/i/İ, ş/s …).
- **Anında açılış:** Uygulama arka planda (tepside) hazır bekler. Genel kısayol pencereyi her yerden açar ve kapatır. Başka bir yere tıklayınca ya da bir dosyayı açınca pencere kendiliğinden gizlenir.
- **Klavye odaklı:** Seçim, gezinme, işlemler ve menüler tek tuşla çalışır. Fareye uzanmanız gerekmez.
- **Hafif:** Kurulum gerektirmeyen tek bir `.exe` dosyasıdır.

## Özellikler

### Arama ve modlar (F1)
Arama kutusunun solundaki çip arama modunu gösterir. **F1**'e basarak ya da çipe tıklayarak modu değiştirirsiniz: **İsim → Uzantı → İçerik → İsim**.
- **İsim:** Dosya ve klasör adlarında arama yapar (birden çok kelime yazılabilir).
- **Uzantı:** Bir uzantı yazıp Enter'a basarsınız (ör. `pdf`); sonuçlar o türle sınırlanır. Çipteki ✕ süzgeci kaldırır.
- **İçerik:** Metin dosyalarının içinde arar. Sonuçlar akarak gelir, en yeni dosyalar önce. Eşleşen satırın numarası ve kendisi görünür.
- Arama kutusu boşken **son kullanılanlar** listelenir: açtığınız öğeler, Windows'un son kullanılanları ve son değişen dosyalar, tek bir listede.

### Klasörlerde gezinme
- Bir klasörde Enter'a basmak ya da çift tıklamak içine girer. Üstteki yol şeridinde (breadcrumb) her parçaya tıklanabilir.
- **Ctrl+←** ya da farenin geri düğmesi üst klasöre çıkar. **Esc** bir adım geri gider.
- Klasör görünümünde **en son değiştirilen en üsttedir**. Yeni oluşturulan dosya ya da not hemen en üstte ve seçili görünür.

### Tab menüsü ve sağ tık menüsü
- **Tab:** Seçili satırın yanında küçük bir işlem kartı açılır. Her satırın bir harf tuşu vardır, harfe basınca o işlem çalışır. En fazla 12 satır görünür, fazlası kartın içinde kayar, pencere büyümez.
  İçerik: Çoklu seçim (Boşluk), Kopyala (c), Kes (x), Yapıştır (v), Yeni (n), Yeniden adlandır (r), Geri Dönüşüm Kutusu'na gönder (d), Bin'e yolla (b), Yolu kopyala (p), Son hedefler (g), Bin yöneticisi (m), Panoyu temizle (k).
- **Sağ tık** (ya da Shift+F10): Aynı görünümde bir menü açar. En üstte **Aç / Önizle / Explorer'da aç** bulunur. Tab menüsünün yapabildiği her şey ve **Yenile (F5)** de buradadır. Her satırda klavye kısayolu yazar.
- Kopyala/Kes dosyaları Windows panosuna koyar, yani Explorer'a da yapıştırılabilir. Hedef klasöre gidip **Ctrl+V** ile yapıştırırsınız. Aynı adda dosya varsa **Üzerine yaz / Yeni adla / İptal** sorulur.
- Silme her zaman **Geri Dönüşüm Kutusu'na** yapılır, öncesinde onay istenir. Kalıcı silme yoktur.

### Çoklu seçim
- **Ctrl+M** ya da menüdeki "Çoklu seçim" açar. **Boşluk** ya da tık ile öğeleri işaretlersiniz. **Shift+↑/↓** aralık seçer, **Ctrl+A** hepsini seçer.
- Kopyala, Kes, Sil ve Yolu kopyala işaretli öğelerin hepsine uygulanır. **Enter** ile seçilenlerin hepsi açılır.

### Son hedefler (Tab → g)
Son girdiğiniz ve kullandığınız klasörlerin listesidir. Birini seçince o klasöre gidersiniz. Aynı ağacın alt klasörleri listeyi doldurmaz, tek kayıt olarak tutulur.

### Bin yöneticisi
"Bin", dosya ve klasörlere terminalden adıyla ulaşmanızı sağlayan küçük başlatıcıların (`.cmd`) bulunduğu bir klasördür.
- **Bin'e yolla (b):** Seçili öğe için bir takma ad yazarsınız. Sonra terminalde o adı yazmak öğeyi açar.
- **PATH'e ekle:** Bin klasörü kullanıcı PATH'ine eklenir. Bu, takma adların yeni açılan terminallerde çalışması içindir; uygulama ilk eklemede bunu sorar.
- **Bin yöneticisi (m):** Takma adları listeler. Buradan yeniden adlandırabilir (F2), kaldırabilir (Del), hedefi açabilir ya da Gezgin'de gösterebilirsiniz.

### Not al (Ctrl+N)
Bulunduğunuz klasöre hızlıca bir metin notu yazarsınız. Enter kaydeder, Shift+Enter yeni satır açar. Not `not-YYYY-AA-GG-SSDD.txt` adıyla kaydedilir ve hiçbir dosyanın üzerine yazmaz. Esc ile çıkarsanız taslak saklanır.

### Terminal (Ctrl+T)
Bulunduğunuz klasörde terminal açar. Ayarlardan seçilebilenler: Windows Terminal, PowerShell, Komut İstemi (CMD), Hyper ya da başka bir program. Ayarlarda yalnızca bilgisayarda bulunanlar listelenir.

### Önizleme (Shift+Space)
Seçili öğenin önizlemesini açar: metin ve kod dosyaları (içerik aramasında eşleşen satır vurgulanır), resimler, zip/jar içeriği ve klasör içeriği. Boyut ve tarih bilgisi de gösterilir.

### Geri Dönüşüm Kutusu
- "çöp", "geri dönüşüm", "recycle" ya da "bin" diye aratınca ve Masaüstü'nde gezinirken çıkan sanal bir klasördür. Dizine eklenmez, her açılışta yeniden okunur.
- İçinde yalnızca **sizin** silinmiş öğeleriniz listelenir. Her birinin gerçek adı, özgün klasörü ve silinme tarihi görünür; en son silinen en üsttedir.
- Menü: **Geri yükle**, Aç, Özgün klasörü aç, Özgün yolu kopyala, Gezgin'de aç.
- Geri yükleme **hiçbir dosyanın üzerine yazmaz**. Aynı adda dosya varsa `ad (2).uzantı` olarak geri yükler. Özgün klasör artık yoksa yeniden oluşturur.
- **Kalıcı silme yoktur.**

### Ayarlar (Ctrl+S ya da sağ alttaki dişli)
- **Genel kısayol:** Varsayılan F9; başka bir tuş kombinasyonuyla değiştirilebilir.
- **Windows açılışında başlat:** Bilgisayar açılınca arka planda hazır bekler.
- **Terminal seçimi.**

Tepsi simgesinin menüsünde Göster ve Çıkış seçenekleri bulunur.

### Yalnız fareyle kullanım
Alt çubuktaki ipuçlarının hepsi tıklanabilir (Aç, Önizle, İşlem, Not, Terminal, Gezgin, Ayarlar …). Mod çipi, yol şeridi, dişli ve menülerin hepsi fareyle çalışır. Fare tekerleği listeleri ve menüleri kaydırır. Çift tık açar, sağ tık menüyü açar.

## Klavye kısayolları

| Tuş | İşlem |
|---|---|
| F9 (değiştirilebilir) | Pencereyi her yerden aç/kapat |
| Yazmak | Ara |
| F1 | Arama modu: İsim → Uzantı → İçerik |
| ↑ / ↓, PageUp / PageDown, Home / End | Listede gezin |
| Enter | Aç / klasöre gir (çoklu seçimde seçilenlerin hepsini aç) |
| Alt+Enter | Explorer'da göster |
| Shift+Space | Önizleme |
| Tab | İşlem menüsü (harf tuşlarıyla) |
| Sağ tık / Shift+F10 | Sağ tık menüsü |
| Ctrl+← / fare geri düğmesi | Üst klasör |
| Esc | Geri / kapat |
| Ctrl+C / Ctrl+X / Ctrl+V | Kopyala / Kes / Yapıştır |
| Ctrl+Shift+C | Yolu kopyala |
| F2 | Yeniden adlandır |
| Del | Geri Dönüşüm Kutusu'na gönder (onaylı) |
| Ctrl+M | Çoklu seçim aç/kapat |
| Boşluk (çoklu seçimde) | İşaretle |
| Shift+↑ / ↓ | Aralık seç |
| Ctrl+A (çoklu seçimde) | Tümünü seç |
| Ctrl+N | Not al |
| Ctrl+T | Terminal aç |
| Ctrl+S | Ayarlar |
| F5 | Dizini yenile |

## Kurulum ve çalıştırma
1. [Releases](../../releases) sayfasından `Fastplorer.exe` dosyasını indirin.
2. Çift tıklayıp çalıştırın. Kurulum gerekmez. Windows 10 ve 11 için yapılmıştır.
3. İlk açılışta sürücüler arka planda dizinlenir; tarama sürerken de arama yapabilirsiniz.

## Kaynaktan derleme
Rust (2024 edition) gerekir.
```
cargo build --release                       # Windows'ta
cargo zigbuild --release --target x86_64-pc-windows-gnu   # Linux'tan çapraz derleme (cargo-zigbuild + zig)
cargo test
```

---

## English

**Fastplorer is a fast file explorer and file search tool for Windows.** Press a global hotkey (default **F9**) and a small window pops up. Results appear as you type. From there you can open, browse, copy/move, rename, or restore from the Recycle Bin, keyboard-first. Mouse-only use works too.

**Why it's fast:**
- Fixed drives are indexed in the background by a parallel scanner into a compact in-memory index. The index is cached on disk and loads instantly on the next start. You can search while scanning is still running.
- Every keystroke runs a multi-threaded ranked search: prefix, word start, contains, then fuzzy. Turkish letters are folded.
- The app waits in the tray, so the hotkey shows it instantly. It hides itself when you click elsewhere or open a file.
- Everything is keyboard-driven: one-key actions in menus.
- It's a single lightweight `.exe` with no installer.

**Features:**
- **F1 search modes:** Name, Extension, and Content. Content search streams results newest first, with line numbers.
- **Recent items** appear when the search box is empty.
- **Folder browsing:** newest-modified first, clickable breadcrumb path, Ctrl+← or the mouse back button goes up.
- **Tab action card and right-click menu:** one-letter keys. The right-click menu also has Open, Preview, Show in Explorer, and Refresh (F5).
- **Copy/cut/paste** through the Windows clipboard, with conflict handling (overwrite / new name / cancel).
- **Delete** always goes to the Recycle Bin after confirmation.
- **Multi-select:** Ctrl+M, Space, Shift+↑/↓, Ctrl+A.
- **Recent destinations** (Tab → g).
- **Bin manager:** the Bin folder holds small `.cmd` launchers that open files from a terminal by alias. "Add to PATH" makes them work in new terminals.
- **Quick notes** (Ctrl+N): never overwrite existing files.
- **Open a terminal here** (Ctrl+T): Windows Terminal, PowerShell, CMD, Hyper, or a custom program.
- **Preview** (Shift+Space): text/code, images, zip contents, folders.
- **Recycle Bin view:** search "recycle"/"bin" or browse the Desktop. It lists your deleted items with original folder and deletion date, newest first. **Restore** never overwrites (`name (2).ext`) and recreates missing folders. There is no permanent delete.
- **Settings** (Ctrl+S or the gear): global hotkey, start with Windows, terminal choice.
- **Mouse-only use:** every hint in the bottom bar is clickable, and lists and menus scroll with the mouse wheel.

**Install:** download `Fastplorer.exe` from Releases and run it. No installer is needed; it targets Windows 10/11.
**Build:** `cargo build --release` on Windows, or `cargo zigbuild --release --target x86_64-pc-windows-gnu` from Linux.
