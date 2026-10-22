# Yerelleştirme sözlüğü (Türkçe)

Bu sözlük, çevirilerin tutarlı kalması için Air arayüzündeki temel terimleri tanımlar.

`app_en.arb` şablonu her metnin ne söylediğini belirler. Bu sözlük, bunu hangi terimle söylediğini belirler. Şablon ile sözlük çelişirse ikisinden birini sessizce seçmeyin, çelişkiyi bildirin.

## Varyant

Türkiye Türkçesi kullanılır ve Türk Dil Kurumunun yazım kuralları geçerlidir. Dil seçicide dilin adı “Türkçe” olarak görünür.

Bir dil varyantı kendi ARB dosyasını ve kendi sözlüğünü alır. Temel dille karıştırılmaz.

## Hitap

Air, kullanıcıya bütün metinlerde “siz” diye hitap eder. Android, Apple, Microsoft, Telegram ve Element de böyle yapar. Hitap metinden metne değişmez. Üslup sade ve sıcak kalır. Resmî kalıplardan (“…mektedir”, “lütfen … ediniz”) kaçının.

Açıklamalarda ve hata mesajlarında emir kipi “-in” ekiyle yazılır: “Tekrar deneyin.”, “Kodu girin.”, “Kopyalamak için dokunun”. “-iniz” biçimi (“deneyiniz”, “giriniz”) kullanılmaz, çünkü resmî ve mesafeli okunur.

Düğmeler, menü öğeleri ve eylem başlıkları fiil köküyle yazılır: “Gönder”, “Sil”, “Kaydet”, “Engelle”, “Tekrar dene”. Türkçe arayüzlerde bu kalıp yerleşiktir ve siz hitabıyla çelişmez. Ayar satırları ve bölüm başlıkları isimdir: “Okundu bilgisi”, “Hesap”.

Soru biçimindeki onay başlıkları edilgen soru olarak yazılır: “Sohbet silinsin mi?”, “Mesaj silinsin mi?”, “{displayName} engellensin mi?”. Onay penceresinin metni “…istediğinizden emin misiniz?” kalıbını kullanır.

İngilizce metinde “please” yoksa çeviriye “lütfen” eklenmez.

Olay bildirimleri ve sistem mesajları görülen geçmiş zamanla yazılır: “{user1}, {user2} kullanıcısını ekledi.”, “Bu mesajı sildiniz.”

## Büyük harf

İngilizce değil, Türkçe yazım kuralları geçerlidir. Başlıklarda ve düğmelerde yalnızca ilk kelime ve özel adlar büyük harfle başlar, İngilizce şablon her kelimeyi büyük yazsa bile: “Davet kodları”, “Güvenlik kodu”. Air, Enter gibi tuş adları ve Kullanım Koşulları gibi belge adları büyük harfle kalır.

Ay ve gün adları belirli bir tarih bildirmiyorsa küçük harfle yazılır: “gelecek pazartesiye kadar”. Belirli bir tarih bildiren ay ve gün adları büyük harfle başlar: “22 Ekim 2026 Perşembe”. Tarihleri uygulama biçimlendirir.

“i” harfinin büyüğü “İ”, “ı” harfinin büyüğü “I” olur. Şablonda tamamen büyük harfle yazılan metin, çeviride de büyük harfle yazılır: “ALAN ADI”. Uygulama Türkçe büyük harf kurallarını bilmez. Bu yüzden büyük harfli metin dosyaya doğrudan büyük harfle yazılır.

## Noktalama

Tırnak işareti “…” biçimindedir. İngilizcedeki "..." ve “...” çeviride “...” olur. `deleteAccountScreen_confirmationInputLabel` bir istisnadır. Oradaki “delete” kelimesi çevrilmez, çünkü uygulama yazılan kelimeyi tam olarak bununla karşılaştırır.

Özel adlara gelen ekler kesme işaretiyle (') ayrılır: “Air'i”, “Air'de”. Yer tutuculara hiçbir ek gelmez. Ayrıntılar “Adlar, yer tutucular ve ekler” bölümündedir.

Metin, kaynak metinle aynı noktalama işaretiyle biter. Bu işaret nokta, soru işareti, ünlem ya da iki nokta olabilir. Kaynakta işaret yoksa çeviride de olmaz. Üç nokta kaynakla aynı yazılır. Kaynakta ayrı ayrı üç nokta “...” varsa çeviride de vardır. Tek karakterlik “…” varsa çeviride de o vardır.

Soru eki “mı, mi, mu, mü” ayrı yazılır: “Sohbet silinsin mi?”

Menü yolu ayracı kaynaktakiyle aynı kalır, örneğin “>” ya da “→”.

Bir cümlede düğme adı geçiyorsa ad tırnak içinde yazılır. Eki, ardından gelen “düğmesi” ismi alır: “Cihaz bağla” düğmesine dokunun. Menü yolundan sonra “bölümü” gelir: Siz > Cihazlar bölümüne gidin.

## Sayılar ve birimler

Sayı ile birim arasında boşluk vardır: “5 MB”. Bayt birimleri Latin harfleriyle yazılır: B, KB, MB, GB, TB, PB, EB, ZB, YB. Ondalık ayracı virgüldür ve uygulama biçimlendirir.

Dakika için zaman damgası “{count} dk” biçimindedir, sonunda nokta yoktur.

Sayılara ek getirilmez. Eki, sayıdan sonra gelen isim alır: “{count} gün içinde”. Tarih ve saat için de aynı kural geçerlidir: “{date} {time} tarihinde”.

## Çoğul

Türkçe, CLDR'de `one` ve `other` kategorilerini kullanır. Sayıdan sonra isim tekil kalır: “1 cihaz”, “5 cihaz”. Bu yüzden `one` ve `other` kollarının metni aynı olacaksa yalnızca `other` kolu yazılır. Kaynakta farklı ifade edilen `=0`, `=1` ve `=2` kolları korunur. `other` kolu zorunludur ve eksikse denetim başarısız olur. `=0` kolu, daha iyi okunuyorsa sayısız yazılır: “Bağlı cihaz yok.”

## Adlar, yer tutucular ve ekler

Türkçede durum ekleri bir adın nasıl okunduğuna göre değişir: Ali'yi, Mehmet'i, Can'ı, Gül'ü. Uygulama yer tutucuya hangi adın geleceğini bilmez. Yabancı adların, emojilerin ve rakamların okunuşu yazılışından tahmin edilemez. Bu yüzden:

- Yer tutucuya hiçbir ek gelmez. `just check-l10n` denetimi, yer tutucudan hemen sonra harf veya kesme işareti gelen çevirileri reddeder.
- Eki, yer tutucudan sonra gelen genel bir isim alır: “{displayName} kullanıcısını”, “{groupName} grubunda”, “{deviceName} cihazının”, “{username} kullanıcı adına”, “{date} tarihinde”.
- Kişiler için genel isim “kullanıcı”dır. İngilizce metin “contact” diyorsa “kişi” kullanılır: “{displayName} adlı Air kişinizi”. “Kişi” kelimesi “kişi ekleme isteği” teriminde de geçtiği için başka yerlerde genel isim olarak kullanılmaz.
- Grup adından sonra “grubu” ya da “grup sohbeti”, sohbet adından sonra “sohbeti”, cihaz adından sonra “cihazı”, kullanıcı adından sonra “kullanıcı adı” gelir.
- Kısa biçim varsayılandır: “{displayName} kullanıcısını”. Genel isim tek kelimeden uzunsa ya da İngilizce “named” diyorsa “adlı” kullanılır: “{displayName} adlı Air kişinizi”, “{device} adlı yeni bir cihazı”.
- Ad cümlenin öznesiyse ek almaz ve genel isme gerek yoktur: “{displayName} kişi ekleme isteğinizi kabul etti.”
- Özne bir adsa ve ardından başka bir ad gelirse, özneden sonra virgül konur: “{user1}, {user2} kullanıcısını ekledi”.
- Soru biçimindeki onay başlıklarında ad, edilgen sorunun öznesi olur: “{displayName} engellensin mi?”. Bu kalıp yalnızca onay başlıklarında kullanılır. Diğer metinlerde cümle İngilizcedeki yapısını korur, çünkü edilgen yapı anlamı kaydırabilir.
- Yer tutucudan sonra gelen “ile” ayrı yazılır, ek olarak yazılmaz: “{emoji} ile tepki verdi”. Genel isimden sonra ek olarak yazılabilir: “{displayName} kullanıcısıyla”.
- `{termsOfUseLink}` ve `{settingsLink}` yer tutucuları kendi anahtarlarındaki bağlantı metniyle doldurulur. Cümlenin gerektirdiği ek bu bağlantı metninin içine yazılır: “Kullanım Koşullarımızı”.

## Air

“Air” çevrilmez. Ekler kesme işaretiyle ayrılır ve “i, e” ünlülerini alır: “Air'i”, “Air'e”, “Air'de”, “Air'den”, “Air'in”. “ile” ayrı yazılır: “Air ile”. Tamlamalarda “Air” ek almaz: “Air hesabı”, “Air kişisi”, “Air kişileriniz”. İngilizce “on Air” “Air'de”, “Join Air” “Air'e katıl”, “Contact Air” ise “Air ile iletişim” olur.

## Temel terimler

| Terim | Anlam | Bağlam |
|-------|-------|--------|
| **Air** | Uygulamanın adı | Çevrilmez. Ekler kesme işaretiyle ayrılır: “Air'de” |
| **Hesap** | Kullanıcının Air hesabı | “Air hesabı” |
| **Kullanıcı adı** | Başkalarının sizinle iletişime geçmek için kullandığı benzersiz ad | Yalnızca kişi bulmak için. Çoğulu “kullanıcı adları” |
| **Görünen ad** | Sohbetlerde başkalarının gördüğü ad | Kullanıcı adından farklıdır |
| **Sohbet** | İki ya da daha fazla kişi arasındaki yazışma | İngilizce “chat”. “Konuşma” değil |
| **Kişi** | Mesaj gönderebileceğiniz başka bir kullanıcı | İngilizce “contact”. “Air kişisi”. “Rehber” değil |
| **Kişi, kişiler** | İngilizce “person, people” | Sayılarda ve “Kişi ekle” düğmesinde: “5 kişi” |
| **Üye** | Grup sohbetindeki katılımcı | Gruplarda |
| **Kullanıcı** | Air'i kullanan herhangi biri | Yer tutucudan sonraki genel isim |
| **Uygulama** | İngilizce “app” | |

## Mesajlaşma

| Terim | Anlam | Bağlam |
|-------|-------|--------|
| **Mesaj** | Sohbette gönderilen metin, görsel veya dosya | “İleti” değil |
| **Benden sil**, **Herkesten sil** | Mesajı yalnızca kendi cihazlarınızdan veya sohbetteki herkesten silmek | |
| **Kime:** | Paylaşılan içeriğin gönderileceği sohbetler | İngilizce “To:” |
| **Taslak** | Yazılmış ama gönderilmemiş mesaj | Sohbet listesinde |
| **Ek** | Mesajla birlikte gönderilen dosya veya görsel | |
| **Düzenle** | Gönderilmiş bir mesajı değiştirmek | Mesajdaki işaret “Düzenlendi” |
| **Yanıtla** | Belirli bir mesaja yanıt vermek | |
| **Tepki** | Bir mesaja emoji eklemek | Fiil “tepki ver”, geçmiş zaman “tepki verdi” |
| **Emoji** | Resimli karakter | Çoğulu “emojiler” |
| **Grup** | Birden çok üyesi olan sohbet | Sohbetin kendisi “grup sohbeti”dir |

## Eylemler ve arayüz

| Terim | Anlam | Bağlam |
|-------|-------|--------|
| **Kişi ekleme isteği** | Kullanıcı adı veya ortak bir grup üzerinden gönderilen istek | İngilizce “contact request”. Bağlam açıksa kısaca “istek”: “İsteği geri çek” |
| **İletişime geçmek** | İngilizce “connect”, kişiler için | Kişiler için asla “bağlanmak” veya “bağlantı kurmak” değil |
| **Ekle** | Bir kişiyi gruba veya kişilerinize eklemek | |
| **Çıkar** | Bir kişiyi gruptan çıkarmak | İngilizce “remove”, yalnızca kişiler için |
| **Kaldır** | Kullanıcı adı, tepki veya kullanılmış kodlar için “remove” | Kişiler için “çıkar” |
| **Sil** | İngilizce “delete” | İçeriği kalıcı olarak silmek: mesaj, sohbet, kişi, hesap. “remove” karşılığı değildir |
| **Ayrıl** | Kendi isteğinizle gruptan çıkmak | Başka birini çıkarmaktan farklıdır |
| **Engelle** | Birinden mesaj almayı durdurmak | |
| **Engeli kaldır** | Engellemeyi geri almak | Düğmede ve metinde aynı ifade |
| **Sessize al** | Bir sohbetin bildirimlerini durdurmak | |
| **Sesi aç** | Sessize almayı geri almak | |
| **Bağla** | Başka bir cihaza hesaba erişim vermek | Yalnızca cihazlar için. “Bağlı cihazlar”. Ağ ve sunucu bağlantısı da “bağlantı”dır |
| **Bağlantıyı kaldır** | Bağlı bir cihazın erişimini geri almak | Yalnızca cihazlar için |
| **Bağlantı** | Bir web adresine giden İngilizce “link” | “Bağlantı açılsın mı?”. Cihaz bağlamayla karışmaz, çünkü fiil olarak kullanılmaz |
| **Kabul et** | Kişi ekleme isteğini kabul etmek | |
| **Reddet** | Kişi ekleme isteğini reddetmek | Gönderen bilgilendirilmez |
| **Geri çek** | Gönderdiğiniz kişi ekleme isteğini geri almak | |
| **Spam bildir** | Birini istenmeyen mesaj gönderen biri olarak işaretlemek | Moderasyon |
| **Dokunun** | İngilizce “tap” | Telefon ve tabletler. Başka bir cihazı anlatan metinde de “dokunun” kalır |
| **Tıklayın** | İngilizce “click” | Masaüstü, `_desktop` anahtarlarında |

## Dosyalar ve veriler

| Terim | Anlam | Bağlam |
|-------|-------|--------|
| **Bayt birimleri** | B, KB, MB, GB, TB, PB, EB, ZB, YB | Sayı ile birim arasında boşluk |
| **Yükle** | Dosyayı sunucuya göndermek | İngilizce “upload” |
| **Görseller** | Fotoğraf arşivinden görsel seçmek | Yeni fotoğraf için “Kamera” |

## Durum ve zaman

| Terim | Anlam | Bağlam |
|-------|-------|--------|
| **Şimdi** | Şu an | Çok yeni bir mesajın zaman damgası |
| **{count} dk** | Birkaç dakika önce | Kısa, noktasız |
| **Bugün**, **Dün** | Bugünkü ve dünkü gün | Mesaj listesindeki tarih ayraçları |
| **Gönderiliyor** | Mesaj sunucuya gidiyor | Mesaj durumu |
| **Gönderilemedi** | Mesaj gönderilemedi | Mesaj durumu |
| **Gönderildi** | Mesaj sunucuya ulaştı | Mesaj durumu |
| **İletildi** | Mesaj alıcının cihazına ulaştı | Mesaj durumu |
| **Okundu** | Alıcı mesajı okudu | Mesaj durumu |
| **Okundu bilgisi** | Okundu durumunun paylaşılıp paylaşılmayacağını belirleyen ayar | |
| **Sona erer** | Kişi ekleme isteğinin süresinin dolması | “3 gün içinde sona erecek”, “Kişi ekleme isteğinin süresi doldu.” |

## Ayarlar ve yardım

| Terim | Anlam | Bağlam |
|-------|-------|--------|
| **Profil ve ayarlar** | Ayarlar ekranı | |
| **Siz** | İngilizce “You” sekmesi | Üye listesinde ve tepkilerde de |
| **Kendime notlar** | Kendi cihazlarınız arasındaki not sohbeti | İngilizce “Notes to self” |
| **Güvenlik kodu** | Sohbetin dinlenmediğini doğrulamak için iki kişinin karşılaştırdığı kod | |
| **Uçtan uca şifreleme** | İngilizce “end-to-end encryption” | Sıfat “uçtan uca şifreli” |
| **Davet kodu** | Air'e katılmak için gereken kod | Her zaman “davet kodu” |
| **Cihazlar** | İngilizce “Devices” satırı | |
| **Bağlı cihazlar** | Aynı hesaba bağlı diğer cihazlar | Bu cihaz “Bu cihaz”dır |
| **QR kodu** | Cihaz bağlamak için kod | |
| **Sayısal kod** | Cihaz bağlamak için rakamlardan oluşan kod | |
| **Sunucu** | Hesabın barındırıldığı bilgisayar | İngilizce “server” |
| **İstemci** | Bir cihazdaki Air uygulaması | İngilizce “client”, teknik metinlerde |
| **Bildirimler** | İngilizce “notifications” | “push notifications” “anlık bildirimler” olur |
| **Günlük kayıtları** | Tanılama kayıtları | İngilizce “logs” |
| **Yardım** | Yardım ve destek | |
| **Air ile iletişim** | Destek ekibiyle iletişim | |
| **Lisanslar** | Açık kaynak bileşenlerinin yasal bilgileri | |
| **Sürüm bilgisi** | Uygulama sürümünün teknik ayrıntıları | |
| **Tamam** | İngilizce “Okay” düğmesi | |

## Çevirmen için

- **Air** çevrilmez. Ekler kesme işaretiyle ayrılır: “Air'i”, “Air'de”
- **Kullanıcı adı** kişileri bulmaya, **görünen ad** onları sohbetlerde tanımaya yarar
- **Çıkar**, **kaldır** ve **sil** İngilizce fiili izler. “remove” kişiler için “çıkar”, diğer her şey için “kaldır” olur. “delete” her zaman “sil” olur
- **İletişime geçmek** ve **kişi ekleme isteği** kişiler içindir, **bağlamak** cihazlar içindir. Kavramlar karıştırılmaz
- **Engelle** ve **engeli kaldır** her metinde aynıdır, pencere metinlerinde de
- Yer tutucuya ek gelmez. Eki bir genel isim alır: “{displayName} kullanıcısını”
- Hitap her zaman “siz”dir, emir kipi “-in” ekiyle yazılır: “deneyin”
- İngilizce metin değiştiyse eski çeviriyi düzeltmeyin, güncel metinden yeniden çevirin
