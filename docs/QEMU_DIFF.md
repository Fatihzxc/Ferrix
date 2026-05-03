# QEMU_DIFF.md — Zynq-7000 Donanımı vs `qemu-system-arm -M xilinx-zynq-a9`

> TikOS geliştirme sırasında karşılaştığımız ve **doğruladığımız** QEMU
> sapmalarının envanteri. Tek amacı: ileride özel (custom) bir QEMU
> makine modeli ya da silikon-uyumlu bir simülatör yazarken her maddenin
> bağımsız bir iş paketi olarak ele alınabilmesi.
>
> Bu dosya **spekülasyon değildir.** Her giriş ya QEMU kaynak kodundan
> (`hw/misc/zynq_slcr.c`, `hw/char/cadence_uart.c` vb.) ya da fiziksel
> donanım üzerinde gözlemden doğrulanmıştır. Doğrulama atıfı her
> maddenin sonundadır.
>
> Yeni madde eklerken **D-NN** numarasını sırayla artır; eski maddelerin
> numarasını yeniden kullanma — başka dokümanlardan referans veriliyor.

## Format

Her madde aşağıdaki şablonu izler:

- **Ne** — sapmanın tek cümlelik tanımı.
- **QEMU davranışı** — model şu an ne yapıyor; ilgili kaynak dosya/fonksiyon.
- **Gerçek donanım** — UG585 / silikon nasıl davranıyor.
- **TikOS'a etkisi** — kodumuzun hangi satırı veya hangi davranışı bu farktan etkileniyor.
- **Custom QEMU'da yapılacak** — düzeltme için somut adım.
- **Kaynak** — atıf zinciri (UG585 sayfası, vault notu, repo dosyası, QEMU kaynağı).

---

## D-001 — `IO_PLL` çıkış formülü ters çevrilmiş

- **Ne:** QEMU PLL çıkışını **bölerek**, gerçek silikon **çarparak** üretir.
- **QEMU davranışı:** `zynq_slcr_compute_pll()` içinde `return input / mult;`
  (`mult = PLL_FDIV`). Yani `33.333 MHz / 30 ≈ 1.11 MHz` üretir.
- **Gerçek donanım:** UG585 s.693, *"33.33 MHz × 40 = 1.33 GHz"*.
  `IO_PLL_out = PS_CLK × PLL_FDIV`. Bizim `FDIV=30` ile çıkış 1000 MHz olur.
- **TikOS'a etkisi:** Doğrudan görünür değil — QEMU UART chardev backend baud
  hızını yok saydığı için bizim baud hesabımız QEMU'da çıktıyı bozmuyor
  (bkz. D-005). Ama saat zinciri matematiği kafayı karıştırıyor: aynı kod
  iki dünyada **çok farklı** nominal frekanslarda çalışıyormuş gibi görünür.
- **Custom QEMU'da yapılacak:** `compute_pll()`'i UG585 formülüne çek
  (`return input * mult;`). Bypass ve reset bit davranışı zaten doğru, sadece
  matematik düzeltmesi.
- **Kaynak:** QEMU `hw/misc/zynq_slcr.c::zynq_slcr_compute_pll()` ·
  UG585 s.693 (PS clock subsystem) · vault: [[M2.5 - QEMU vs Gerçek Donanım#IO_PLL Saat Hesaplama]].

---

## D-002 — `PLL_STATUS` her zaman `0x3F` (kilit anında)

- **Ne:** QEMU tüm PLL'leri her zaman "kilitli + stabil" gösterir; gerçek
  silikon kilitlenme süresine ihtiyaç duyar.
- **QEMU davranışı:** `PLL_STATUS` register'ı sabit `0x3F` döner. `LOCK` bit'leri
  ([3:1]) ve `STABLE` bit'leri ([7:5,4]) her zaman 1.
- **Gerçek donanım:** PLL `RESET=0` yapıldıktan sonra kilit alana kadar
  (~10–100 µs, donanım/voltaj bağımlı) `LOCK` biti 0 kalır. UG585 s.1582-1583.
- **TikOS'a etkisi:** `io_pll_configure()` içindeki `wait_lock()` busy-wait
  döngüsü QEMU'da **ilk iterasyonda** çıkar. Timeout dalını QEMU testinde
  asla tetikleyemeyiz — sadece JTAG-park gerçek donanım testi kanıtlar.
- **Custom QEMU'da yapılacak:** PLL state machine ekle (BYPASS/RESET/LOCKING/
  LOCKED) + sanal "kilit gecikmesi" (örn. 1000 sanal saat çevrimi). `RESET=0`
  yazıldığında `LOCK` biti gecikmeli olarak set edilsin.
- **Kaynak:** QEMU `hw/misc/zynq_slcr.c` (PLL_STATUS sabiti) ·
  UG585 s.1582-1583 · vault: [[M2.5 - PLL Lock Polling ve Timeout]],
  [[M2.5 - QEMU vs Gerçek Donanım#IO_PLL Saat Hesaplama]].

---

## D-003 — MIO pin mux simüle edilmemiş

- **Ne:** QEMU `MIO_PIN_XX` register'ına yazılan değeri saklar ama L0–L3
  mux'unun sinyal yönlendirmesini hiç simüle etmez.
- **QEMU davranışı:** UART modeli (chardev) doğrudan SoC'ye bağlı; MIO
  yapılandırmasına bakmadan her zaman aktiftir. `MIO_PIN_48`'e farklı bir
  L3_SEL yazsan da UART chardev üzerinden çalışmaya devam eder.
- **Gerçek donanım:** UART1 TX/RX sinyalleri MIO48/49'a `L3_SEL=7` ile
  bağlanır. Yanlış mux = pin başka bir çevre birimine yönlenir → UART
  fiziksel olarak ölü.
- **TikOS'a etkisi:** `mio_route_uart1()` helper'ı QEMU'da **etkisiz** ama
  zararsız. Helper'ı QEMU testinde çalıştırmak regresyon üretmez (yazılır,
  unutulur), gerçek donanımda kritik.
- **Custom QEMU'da yapılacak:** Her MIO pini için "şu an hangi periferiğe
  bağlı" durumu modelle. UART/SPI/I2C modelleri pin durumuna baksın; pin
  başka bir periferiğe yönlendirildiyse UART chardev'i kapansın.
- **Kaynak:** QEMU `hw/misc/zynq_slcr.c` (register saklama, mux mantığı yok) ·
  UG585 Tablo 4-2, Appendix B · vault:
  [[M2.5 - QEMU vs Gerçek Donanım#MIO Pin Yönlendirme]],
  [[M2.5 - MIO Pin Yönlendirme]].

---

## D-004 — `MST_TRI` (tri-state) simüle edilmemiş

- **Ne:** QEMU `MST_TRI0/1` register'ını saklar ama tri-state davranışını
  uygulamaz; tüm pinler her zaman "aktif" kabul edilir.
- **QEMU davranışı:** Reset değeri `0xFFFFFFFF` (tümü tri-state) doğru
  saklanıyor, ama UART/SPI/GPIO modelleri bu register'a bakmaz.
- **Gerçek donanım:** `MST_TRI[N]=1` iken pin yüksek empedans (Hi-Z); fiziksel
  olarak bağlantısız. Mux doğru olsa bile sürücü kapalı = UART sessiz.
  UG585 s.1687-1688.
- **TikOS'a etkisi:** `mst_tri_clear_uart1()` helper'ı QEMU'da etkisiz;
  gerçek donanımda kritik. D-003 ile aynı pattern: aynı kod, iki dünya,
  bir tarafta no-op.
- **Custom QEMU'da yapılacak:** Pin sürücü modeli: `MST_TRI[N]=1` iken pin
  bağlı olduğu periferiğin çıktısını mask'le (input'a çevir). Çıktı modelleri
  bu maskeden geçsin.
- **Kaynak:** QEMU `hw/misc/zynq_slcr.c` · UG585 s.1687-1688 ·
  vault: [[M2.5 - QEMU vs Gerçek Donanım#MST_TRI (Tri-State)]].

---

## D-005 — UART baud rate chardev backend tarafından yok sayılır

- **Ne:** QEMU UART modeli baud'u doğru hesaplar ve chardev API'sine iletir,
  ama `stdio` (ve diğer ağ-tabanlı) backend'ler bu parametreyi kullanmaz —
  her byte anında iletilir.
- **QEMU davranışı:** `cadence_uart.c::uart_parameters_setup()` `BAUDGEN ×
  (BAUDDIV+1)` formülünü doğrular ve `qemu_chr_fe_ioctl(SERIAL_SET_PARAMS)`
  çağırır; stdio chardev bu ioctl'i no-op olarak işler.
- **Gerçek donanım:** Yanlış `BAUDGEN/BAUDDIV` = bozuk byte'lar (terminalde
  çöp) veya hiç çıktı (host UART aynı baud'a senkronize değilse).
- **TikOS'a etkisi:** `BAUD_CD=124, BAUD_BDIV=6` ile 115207 baud hesabımızı
  QEMU'da test edemeyiz. Doğrulama yalnızca gerçek donanımda
  (`scripts/serial-listen.ps1` 115200 8N1) yapılabilir.
- **Custom QEMU'da yapılacak:** En azından bir "synthetic baud delay"
  modu: chardev'den çıkan her byte'ı baud hesabına göre geciktir. Tam
  bit-seviye simülasyon gerek değil; throughput kontrolü yeterli.
- **Kaynak:** QEMU `hw/char/cadence_uart.c::uart_parameters_setup()` ·
  UG585 s.1785, s.1792 · `src/main.rs:25-26` (kod içi yorum) ·
  vault: [[M2.5 - QEMU vs Gerçek Donanım#UART Baud Rate]].

---

## D-006 — `CLKACT=0` UART modelinde yazmacı erişilemez kılar

- **Ne:** QEMU UART modelinde `refclk` aktif değilse (`UART_CLK_CTRL`
  CLKACT bit'i 0) tüm yazmaç erişimleri **reddedilir** (`MEMTX_ERROR`).
- **QEMU davranışı:** UART modeli `Clock` API üzerinden `uart_ref_clk`
  sinyalini izler. Saat 0 Hz ise `mmio_read/write` hata döner.
- **Gerçek donanım:** UART register'ları her zaman erişilebilir; saat
  yokken iletim yapılmaz ama yazmaç okuma/yazma çalışır.
- **TikOS'a etkisi:** Eğer M2.5 init'inde yanlışlıkla CLKACT bit'lerini
  temizlersek QEMU sert hata verir (data abort), gerçek donanımda
  sessizce ölü UART. **QEMU bu hatayı bizim için erken yakalar** — istemeden
  faydalı bir sapma.
- **Custom QEMU'da yapılacak:** Bu davranışı *değiştirme*; donanımdan
  daha sıkı, ama hata teşhisinde yardımcı. Yine de, "donanım uyumluluk"
  modunda gevşetilebilir bir flag eklenebilir.
- **Kaynak:** QEMU `hw/char/cadence_uart.c` (refclk kontrol) ·
  UG585 s.1594-1595 · vault:
  [[M2.5 - QEMU vs Gerçek Donanım#UART_CLK_CTRL]].

---

## D-007 — BootROM yok; ELF doğrudan yüklenir

- **Ne:** QEMU `-kernel <ELF>` ile başlatıldığında BootROM çalıştırmaz;
  ELF'i program header'larına göre yükler ve PC'yi entry'ye koyar.
- **QEMU davranışı:** Boot mode straps yok sayılır. ELF section'ları
  fiziksel adreslerine kopyalanır. CPU MMU/cache kapalı, TLB temiz.
- **Gerçek donanım:** Power-on → BootROM çalışır → MIO[2:8] strap'lerine
  göre boot mode seçer (JTAG-park, QSPI, SD, NAND, NOR). QSPI modunda
  IO_PLL bringup, QSPI pin config, OCM kopyalama yapılır. JTAG modunda
  minimal config + WFE.
- **TikOS'a etkisi:** Üç farklı başlangıç durumu (QEMU vs JTAG vs QSPI).
  Bizim çözümümüz idempotent başlatma — kod her üç durumda da güvenli
  çalışacak şekilde yazıldı.
- **Custom QEMU'da yapılacak:** Opsiyonel BootROM modeli. Strap pinleri
  oku, JTAG modunda WFE'de park ol; QSPI modunda boot.bin parse et,
  FSBL'i OCM'e kopyala, jump et. Bu ciddi bir iş — `secure boot` (eFUSE)
  hariç tutulabilir.
- **Kaynak:** QEMU `docs/system/arm/xlnx-zynq.md` ·
  UG585 Bölüm 6 (Boot and Configuration) · vault:
  [[M2.5 - Aynı İkili İki Dünya]],
  [[M2.5 - QEMU vs Gerçek Donanım#CPU Başlangıç Durumu]].

---

## D-008 — OCM remap (`OCM_CFG`) dinamik değil

- **Ne:** QEMU `SLCR_OCM_CFG` register'ını saklar ama OCM blok adres
  eşlemesini gerçekten değiştirmez.
- **QEMU davranışı:** OCM 256 KB sabit adres aralığında modellenmiş;
  `OCM_CFG[RAM_HI]` bit'leri yazılabilir ama remap fiziksel olarak
  uygulanmaz.
- **Gerçek donanım:** UG585 s.749, "RAM located at 0x00000000–0x0002FFFF
  can be relocated to 0xFFFC0000". `OCM_CFG[RAM_HI]` bit'leri her 64 KB
  bloğun konumunu belirler. `scripts/load-and-run.ps1` `OCM_CFG=0x1F`
  yazarak tüm 4 bankı `0xFFFC0000`'a taşıyor.
- **TikOS'a etkisi:** Linker `0xFFFC0000`'da çalışıyor; QEMU bu adresi
  zaten OCM olarak görüyor (sabit eşleme). Yani OCM_CFG yazımı QEMU'da
  no-op, gerçek donanımda gerekli — ama OpenOCD scriptinde yapılıyor,
  Rust kodumuzda değil.
- **Custom QEMU'da yapılacak:** OCM_CFG'yi gerçek bir `MemoryRegion`
  remap'ine bağla. RAM_HI=1 → blok `0xFFFCxxxx`'te görünsün; RAM_HI=0 →
  `0x0000xxxx`'te görünsün. Şu an hep 1 gibi davranıyor (tesadüfen
  doğru, ama davranış değil).
- **Kaynak:** QEMU `hw/misc/zynq_slcr.c` · UG585 s.749 ·
  `scripts/load-and-run.ps1` (OCM_CFG=0x1F yazımı) · vault:
  [[M2.5 - QEMU vs Gerçek Donanım#OCM (On-Chip Memory)]].

---

## D-009 — `uart_ref_clk` varsayılanı bağlantısızken 50 MHz

- **Ne:** SLCR clock zinciri UART'a bağlı değilse QEMU UART modeli
  `UART_DEFAULT_REF_CLK = 50 MHz` fallback'ine düşer.
- **QEMU davranışı:** `cadence_uart.c` içinde `Clock` input'u henüz
  bağlanmadıysa 50 MHz kabul edilir. Gerçek hayatta SoC composition
  zamanında bağlanır, ama bağımsız test senaryolarında bu fallback
  görünür.
- **Gerçek donanım:** Böyle bir varsayılan yok. `uart_ref_clk` daima
  SLCR'den türer; SLCR doğru kurulmamışsa UART davranışı tanımsız.
- **TikOS'a etkisi:** Bizim için doğrudan görünür değil — `xilinx-zynq-a9`
  makine modeli SLCR-UART bağlantısını kuruyor. Ama bağımsız bir
  Cadence UART unit-test'inde bu fallback yanıltıcı olabilir.
- **Custom QEMU'da yapılacak:** Varsayılanı kaldır veya assert'e çevir.
  Saat bağlanmadan UART'a yazma → açık hata olsun.
- **Kaynak:** QEMU `hw/char/cadence_uart.c` (`UART_DEFAULT_REF_CLK`) ·
  vault: [[M2.5 - QEMU vs Gerçek Donanım#UART Baud Rate]].

---

## D-010 — CPU başlangıçta MMU/cache zaten kapalı

- **Ne:** QEMU `-kernel` boot'ta CPU MMU kapalı, D/I-cache kapalı, TLB
  temiz. Gerçek donanımda gelen durum boot moduna göre değişir.
- **QEMU davranışı:** SCTLR reset değeri standart ARM v7-A: `M=0`, `C=0`,
  `I=0`. CPU temiz başlar.
- **Gerçek donanım:**
  - **JTAG-park:** OpenOCD CPU'yu durdurur ve SCTLR'i temizler
    (`scripts/load-and-run.ps1` SCTLR'den `M|C|I` bit'lerini siler).
    Resume sonrası CPU temiz.
  - **QSPI:** BootROM kendi MMU/cache yapılandırmasını bırakır; FSBL ya
    da bizim kodumuz bunu temizlemek zorunda.
- **TikOS'a etkisi:** `_start` Assembly'sinde SCTLR'den M/C/I temizleyip
  TLBIALL + ICIALLU çalıştırıyoruz (`src/main.rs:218-249`). QEMU'da bu
  no-op, gerçek donanımda zorunlu. Kasıtlı bir savunma.
- **Custom QEMU'da yapılacak:** Boot mode strap'lerine göre SCTLR
  başlangıç durumunu farklılaştır (JTAG temiz, QSPI BootROM'un bıraktığı
  hâl). Ancak BootROM modellenmedikçe (D-007) anlamsız.
- **Kaynak:** `src/main.rs:218-249` (`_start` SCTLR clear + TLB invalidate) ·
  `scripts/load-and-run.ps1` (OpenOCD SCTLR clear) · vault:
  [[M2.5 - QEMU vs Gerçek Donanım#CPU Başlangıç Durumu]],
  [[M2.5 - Aynı İkili İki Dünya]].

---

## Henüz açılmamış alanlar (placeholder)

Aşağıdaki alanlarda gelecekte sapmalar göreceğimizi tahmin ediyoruz ama
şu an doğrulanmış bir madde **yok**. Yeni bir D-NN açmadan önce kanıt
gerekiyor (kod davranışı + UG585 atıfı).

- **GIC** (M4'te açılacak — Zynq GIC-400, distributor + cpu interface).
- **Private timer** (M5 — Cortex-A9 per-core 32-bit down-counter).
- **Cache coherency** (MMU + L1/L2 sonrası, AMP veya SMP'de SCU/PL310).
- **DDR controller** (DRAM bringup; QEMU şu an DDR'ı sahte 256 MB sunuyor).
- **eFUSE / secure boot** (BootROM'un RSA hash kontrolü).
- **DMA controller** (PL330).

---

## Atıf zinciri

- **UG585** — *Zynq-7000 SoC Technical Reference Manual* (Xilinx).
- **QEMU kaynak** — `hw/misc/zynq_slcr.c`, `hw/char/cadence_uart.c`,
  `docs/system/arm/xlnx-zynq.md` (QEMU repo).
- **Vault notları** — `C:\Users\Fatih\github\obsidian\tikos_vault\TikOS\`
  altındaki Türkçe öğretim notları.
- **TikOS kodu** — `src/main.rs`, `linker.ld`, `scripts/load-and-run.ps1`.
