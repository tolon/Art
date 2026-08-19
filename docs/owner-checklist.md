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
5. **Türkçe paketini** işaretle, önizlemeye bak.
6. Panelin altındaki "ART yalnızca tarifi olan paketleri kurar" cümlesini oku
   — **anlaşılır mı?** Kendi arşivin teklif edilmiyorsa bunun neden olduğunu
   söylüyor mu?

**Tamam sayılır:** Hangi paketlerin sunulduğunu, hangilerinin neden
sunulmadığını ekrana bakarak anlayabildiysen. Bir yerde "bu ne demek şimdi?"
dediysen, orası bir kusurdur — bana söyle.

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
| 3.4 | **VHD/RDB sistem imajı** | RDB'li bir sistem imajıyla bir başlık aç | Açıldı — **ART-146 tam olarak bu yüzden kanıtsız** |
| 3.5 | **Kayıt hayatta kalıyor mu** | `allow_write` açıkken bir oyun oyna, **kaydet**, kapat, tekrar aç | Kayıt duruyor |

**3.5 hakkında bilinen:** İki başlık `allow_write` açıkken oynandı, imajlar
emülatör kapandıktan sonra okundu ve **2021 zaman damgalarını koruyordu** —
yani o iki oyun hiç yazmadı. Bu, özelliğin çalışmadığı anlamına gelmiyor;
*sınanmadığı* anlamına geliyor. **Kaydı olan bir oyun** gerekiyor.

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

## 5. Sırada bekleyen, ama henüz sende olmayan

**BoingBag'li bir ağacın açılması.** İçerik katmanının kapanış çıtasıydı ve
karşılanmadı. Şu an *kimse* yapamaz: BoingBag yükleri şifreli ve yalnızca
paketin kendi Amiga tarafındaki `Updater`'ı açabiliyor (ART-166).

**Amiga tarafı kurulum turu** bitince sende olacak. Unutulmasın diye burada.

---

## Bilinen ve kapanmış olan — bunları tekrar denemene gerek yok

- **AmigaOS 3.9 ağacı açılıyor**, ve gerçekten 3.9: `Workbench 45.1
  (13-Nov-00)`. Kendi CD'nden, 1879 dosya, WinUAE'de temiz.
- **Bir A500/A500+ Gotek'ten önyüklendi** (2026-08-12, fotoğraflı).
- **AmigaOS 3.2 ağacı PFS3 biriminde açıldı**, lisanslı V47 ROM'la, duvar
  kâğıdı dahil.
- **İki oyun oynandı** — bir WHDLoad başlığı ve bir disket başlığı.
