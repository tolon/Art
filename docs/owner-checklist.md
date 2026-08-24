# Senin listen — koddan çıkamayacak işler

**Bu dosya sende olan işler için.** Hepsi tek bir sebeple burada: ART'ın kodu
bunları yapabilir ama **doğru olduklarını kanıtlayamaz.** Kanıt ya gerçek
donanımdan ya da gerçek bir ekranı süren bir insandan gelir.

Sıra önemli: yukarıdan aşağı gidersen her madde bir öncekinin açtığı yolu
kullanır. Her maddenin sonunda **"tamam sayılır"** satırı var — o cümleyi
söyleyebiliyorsan madde bitmiştir.

Sonucu bana söylemen yeterli, ben `FEATURES.md` ve `ISSUES.md`'yi güncellerim.
**Beklediğin şey çıkmazsa da söyle** — bu projede kusurların çoğunu ekranı
süren sen buldun, ve "beklediğim olmadı" en değerli cümle.

---

## 1. Paket ekranını elle sür

**Neden sende:** Bu ekranın tamamı yalnızca bileşen testleriyle kaplı. Dün
kurulum ekranını sürdün ve iki kusur çıktı — sessizce reddeden bir işlem ve
hiçbir şey açıklamayan bir alan. İkisini de hiçbir test bulmamıştı.

**Ne yap:**
1. Programı aç, **İşletim Sistemi Kurucusu → Kur**.
2. Paket klasörünü `E:\amiga\Amigatolon\paketler` yap.
3. Listeye bak: **kaç paket görünüyor, hangileri işaretlenebiliyor?**
4. **BoingBag satırlarına özellikle bak.** İşaretlenememeleri ve yanlarında
   Türkçe bir cümle olması gerekiyor — o cümle paketin kendi Amiga tarafındaki
   `Updater`'ını adlandırmalı.
5. **Türkçe paketini işaretle** ve önizlemeye bak. Bunun ayrı bir hikâyesi
   var: ilk ölçümde seçilemiyordu, çünkü sekiz dil varyantının hepsi arşivin
   içinde `LocaleUpdate` adını taşıyor ve ART hangisinin hangisi olduğunu
   ayırt edemiyordu. Artık kimlik iki şeye bakıyor — üst düzey klasör **artı**
   arşiv içinde bildirilmiş bir yol. Yani **bu satırın seçilebilir olması ve
   Türkçe olanı getirmesi** doğrulanacak şeyin kendisi. Önizlemede
   `Locale/Catalogs/TÜRKÇE` altında dosyalar görmelisin.
6. Panelin altındaki "ART yalnızca tarifi olan paketleri kurar" cümlesini oku
   — **anlaşılır mı?** Kendi arşivin teklif edilmiyorsa bunun neden olduğunu
   söylüyor mu?
7. **Bir kurulum yap ve sonucuna bak.** Sizin diskinizde ölçüldü: 3.9 tabanı
   `workbench-base` + `locale-base` + `workbench-39` ile geliyor, ve
   `workbench-39` katmanı 622 yeni dosya, 19 yükseltme, 0 düşürme üretiyor.
   Önizleme bunu **sınıflara ayırarak** göstermeli, düşürmeler en üstte.

**Tamam sayılır:** Hangi paketlerin sunulduğunu, hangilerinin neden
sunulmadığını ekrana bakarak anlayabildiysen, **ve Türkçe paketi gerçekten
seçilip önizlenebiliyorsa.** Bir yerde "bu ne demek şimdi?" dediysen, orası
bir kusurdur — bana söyle.

---

## 2. Kurulum ekranını gerçek tarayıcıda sür (ART-118)

**Neden sende:** Headless Chrome/Edge bu ekranda **tekrarlanabilir biçimde
çöküyor** ve sebebi bulunamadı. Senin makinende, senin sürmenle daralır.

**Ne yap:** Programı normal aç (headless değil), kurulum ekranında gez:
sürüm seçicisini değiştir, bileşenleri işaretle, medya klasörünü değiştir,
hedef seç. Çökme, donma ya da boş kalan bir bölüm olursa **ne yaptığın anda**
olduğunu not et.

**Tamam sayılır:** Ekranın tamamını çökmeden gezebildiysen — ya da çöktüyse
hangi işlemde çöktüğünü söyleyebiliyorsan. İkincisi birincisinden değerli.

---

## 3. README'nin topluluk test listesi — beş madde

Bunlar `CHANGELOG.md`'de "denenmeyi bekliyor" diye duruyor. Her biri bir
FEATURES satırını sarıdan yeşile çevirir.

| # | Ne | Nasıl | Tamam sayılır |
|---|---|---|---|
| 3.1 | **Çıplak bir `.adf` başlığı** | Koleksiyondan bir disket başlığı seç, **Oynat** | Oyun açıldı |
| 3.2 | **Bir `.rp9` hardfile başlığı** | `.rp9` içinde hardfile olan bir başlık seç, **Oynat** | Oyun açıldı |
| 3.3 | **Y1/Y2 çekmece yolu** | Bir WHDLoad başlığını `E:\amiga\amikit\AmiKit.hdf` ile aç | Oyun açıldı; hangi yolun kullanıldığı ekranda yazıyor |
| 3.4 | **VHD/RDB sistem imajı** | RDB'li ya da VHD'li bir sistem imajıyla bir başlık aç | Açıldı — **yarısı 2026-08-24'te kapandı**, kalan yarısı emülatörü istiyor (aşağıdaki nota bak) |
| 3.5 | **Kayıt hayatta kalıyor mu** | `allow_write` açıkken bir oyun oyna, **kaydet**, kapat, tekrar aç | Kayıt duruyor |

**3.4 hakkında bilinen (2026-08-24):** ART artık senin gerçek 1.2 GB
`AmiKit.hdf`'ini doğrudan okuyor — ilk sekiz baytından yapılmış bir taklidi
değil. İki ayrı okuyucu aynı şeyi söylüyor: **dinamik** bir VHD, içinde 3.9 GB
disk, sağlama toplamı tutuyor; ART'ın yazdığı satırda zorlanmış geometri yok.
**Ama bu, ART'ın yazdığı ayarı ölçüyor — WinUAE'nin onunla ne yaptığını
değil.** Kalan tek soru bu, ve cevabı emülatörü açmakla geliyor.
Koşmak istersen: `ART_REAL_HARDFILE` değişkenine imajın yolunu ver,
`cd src-tauri && cargo test the_real_vhd_gets_no_forced_geometry -- --ignored --nocapture`.

**3.5 hakkında bilinen:** İki başlık `allow_write` açıkken oynandı, imajlar
emülatör kapandıktan sonra okundu ve **2021 zaman damgalarını koruyordu** —
yani o iki oyun hiç yazmadı. Bu, özelliğin çalışmadığı anlamına gelmiyor;
*sınanmadığı* anlamına geliyor. **Kaydı olan bir oyun** gerekiyor.

---

## 3.6 Türkçeyi ekranda gör (ART-062)

**Neden sende:** Bu aşamada inen her Türkçe dize `pnpm test`'in anahtar
denetiminden ve JSON okumasından geçti — ama **1900 anahtarın 1899'u çalışan
uygulamada ekranda hâlâ görülmedi.** Anahtarların eşleşmesi, cümlenin doğru
olduğunu ya da kutuya sığdığını söylemez.

**Bir tanesi görüldü, 2026-08-21:** sürüm derlemesinde eski bir başlık
seçtin ve yeni WHDLoad reddini Türkçe okudun — *"…bu başlatma bir A1200
Kickstart 3.x istiyor…"* — *"gayet makul bir çözüm olmuş"* dedin, ardından
başlatma çalıştı. Bu, ART'ta herhangi bir Türkçe cümlenin dilini bilen biri
tarafından ekranda ilk okunuşu, ve en zor türden biriydi: okuyanı ne
yapacağını bilerek bırakması gereken bir ret. Kalanı hâlâ görülmedi
(ART-062).

**Ne yap:** Yukarıdaki maddeleri yaparken zaten gezeceğin ekranlarda Türkçeye
dikkat et. Özellikle bak:

- **Taşan ya da kesilen** cümle var mı — Türkçe İngilizceden uzundur
- **Çeviri gibi durmayan**, İngilizce cümle yapısıyla yazılmış bir yer var mı
- **Hiç çevrilmemiş** bir şey görüyor musun — özellikle hata mesajları
  (bunların bir kısmı bilinçli İngilizce, ART-060; ama hangileri olduğunu
  görmek istiyorum)
- Rakam ve tarih biçimleri doğru mu

**Tamam sayılır:** Gezdiğin ekranlarda Türkçenin okunabilir olduğunu
söyleyebiliyorsan. Tuhaf duran bir cümle görürsen ekranın adıyla birlikte
söyle — düzeltmesi kolay, bulunması değil.

---

## 4. Donanım — SD-1'in kalan tek basamağı

**Gereken:** microSD kart, USB kart okuyucu, HDMI kablo.

**Neden sende:** Kod tarafında **hiçbir engel yok.** Kartın şekli (MBR + FAT32
+ `0x76` alanları), RDB'ye sürücü gömme, FAT32 boot bölümü, yapı manifestosu,
imaj sağlık kontrolü — hepsi bitti, testli, ve 7-Zip ile bağımsız doğrulandı.
Eksik olan yalnızca fiziksel malzeme.

**Ne yap (malzeme geldiğinde):**
1. ART ile bir kart imajı üret.
2. Kartı yaz (kart yazan yüzlerce program var, ART yazmıyor — bilinçli karar).
3. PiStorm'a tak ve aç.

**Tamam sayılır:** Kart açıldı. **Bu, projenin 1.0 çıtası** — daha büyük bir
sürüm numarası değil, bu.

---

## 5. Amiga tarafı kurulum ekranını elle sür

**Bu madde 2026-08-21'de değişti.** Burada eskiden *"BoingBag'li bir ağacın
açılması — şu an kimse yapamaz"* yazıyordu. Artık oluyor, ve senin kendi
malzemenle ölçüldü: `BoingBag39-1 (1).lha` **169,1 s**'de, ardından onun
üstüne `BoingBag39-2.lha` **138,1 s**'de kuruldu (3 795 → 3 859 → 3 868
dosya), ağaç yalnızca başarıda aslının yerine geçti. Sonra **soruldu, tahmin
edilmedi**: açılan ağaç `version full`'e `Workbench 45.3 (07-Dec-01)`,
`version.library 45.3`, `workbench.library 45.127` cevabını veriyor — eskiden
`Workbench 45.1 (13-Nov-00)` diyordu. **Hiçbir şey şifre kırmıyor**: paketin
kendi `Updater`'ı, emülatörün içinde, kendi yükünü kendisi açıyor (ART-166'nın
duvarı bu yüzden yıkılmadı, dolaşıldı).

**Neden yine de sende:** bunların hepsi `#[ignore]`li bir test kancasından
koştu — `compose` → `install`, komutun kullandığı yolun aynısı, ama **ekrandan
değil.** `AmigaInstallPanel.tsx` bir insanın elinden hiç geçmedi ve bu projede
kusurların çoğu tam orada çıktı.

**Ne yap:**
1. **İşletim Sistemi Kurucusu**'nu aç, Amiga tarafı kurulum paneline gel.
2. 3.9 ağacını ve `BoingBag39-1 (1).lha`'yı seç, **önizlemeye bak** — önizleme
   hiçbir şey başlatmamalı, hiçbir şey yazmamalı.
3. Kur. **Dört sonun hangisi geldiğine bak**: başardı · kurucu reddetti ·
   süre doldu · pencereyi sen kapattın. Dördü dört ayrı cümle ve dört ayrı
   "şimdi ne yap" olmalı; ikisi aynı cümleyi veriyorsa bu bir kusurdur.
4. **Diski takmadan da dene.** 3.9 CD'si bağlı değilken `Updater` kendi
   *"Checking AmigaOS 3.9 CD-ROM…"* satırına gelip AmigaDOS'un
   `Please insert volume AmigaOS3.9` isteğinde duruyor — ART bunu önden
   reddetmeli ve **hiçbir şey kopyalamamalı**.
5. Bir kurulumu **yarıda kes**. Ekran, kendi ağacına dokunulmadığını
   söylemeli; kopyanın atıldığını **iddia etmemeli** eğer çekirdek atamadığını
   söylüyorsa.

**Tamam sayılır:** Paneli baştan sona sürdüysen ve gördüğün her sonun sana ne
yapacağını söylediğini söyleyebiliyorsan. Bir yerde ART'ın yapmadığı bir şeyi
yaptığını söylediğini gördüysen — bu, bu projenin en pahalı kusur sınıfı,
hemen söyle.

---

## 6. Aminet ekranını elle sür

**Neden sende:** Zincirin tamamı gerçek Aminet'e karşı koşuldu ve çalışıyor —
her ayna **ayrı ayrı** soruldu (üçü de ayakta, 85 472 paket, 0 atlanan),
katalog eşitlenip geri okundu, gerçek bir paket indi, kapılardan geçti ve
açıldı. Bunu istediğin zaman kendin koşabilirsin:

```
cd src-tauri && cargo test live_aminet -- --ignored --nocapture
```

Hiç olmayan tek şey **çalışan uygulamada düğmeye basmak.**

**Nasıl:** `/aminet`'i aç → Eşitle → istediğin bir şeyi ara → indir → bir
disket imajına ya da sabit disk bölümüne kur.

**Tamam sayılır:** Katalog doldu, indirilen dosya senin seçtiğin klasöre indi,
ve kurulum neyi nereye koyduğunu söyledi.

**Bana söyle:** Toplamı bilinmeyen bir ilerleme çubuğu gördüysen, ya da bir
ayna hatası hangi ayna olduğunu söylemediyse.

---

## 7. İki AmigaOS'lu bir kart

**Neden sende:** ART böyle bir kart kuruyor artık, ama hangisinin açılacağını
seçen şey **Amiga'nın kendi Early Startup ekranı** — ART menü yazmıyor, çünkü
AmigaOS'ta zaten var. Bunun doğru çalıştığını ancak gerçek bir açılış söyler.

**Nasıl:** Kart kurucusunda Gelişmiş → *"Bu kartta ikinci bir AmigaOS"*, boyut
ver, kur. Sonra makineyi açarken **iki fare düğmesini** birden basılı tut.

**Tamam sayılır:** İkisi de listede, ve hiçbir şey tutmadığında ART'ın yüksek
öncelik verdiği olan açılıyor.

**Bana söyle:** Yalnızca biri göründüyse, ya da yanlış olan açıldıysa. ART sana
ikisinin *eşit öncelikte* olduğunu söylediyse, asıl bildirilecek şey o uyarı.

---

## Bilinen ve kapanmış olan — bunları tekrar denemene gerek yok

- **AmigaOS 3.9 ağacı açılıyor**, ve gerçekten 3.9: `Workbench 45.1
  (13-Nov-00)`. Kendi CD'nden, 1879 dosya, WinUAE'de temiz.
- **İki BoingBag de kuruldu** ve açılan ağaç bunu kendisi söylüyor:
  `Workbench 45.3 (07-Dec-01)`. Senin kendi arşivlerinle, ürünün kendi
  yolundan, 169,1 s + 138,1 s.
- **Bir A500/A500+ Gotek'ten önyüklendi** (2026-08-12, fotoğraflı).
- **AmigaOS 3.2 ağacı PFS3 biriminde açıldı**, lisanslı V47 ROM'la, duvar
  kâğıdı dahil.
- **İki oyun oynandı** — bir WHDLoad başlığı ve bir disket başlığı.
