# Lokaliseerimise sõnastik (eesti keel)

See sõnastik määrab Airi kasutajaliidese põhiterminid, et tõlked püsiksid ühtlased.

Mall `app_en.arb` määrab, mida sõne ütleb. Sõnastik määrab, milliste sõnadega seda öeldakse. Kui mall ja sõnastik lähevad lahku, anna lahknevusest teada ja ära vali vaikides ühte neist.

## Variant

Kasutatakse eesti kirjakeelt kehtiva õigekeelsussõnaraamatu järgi. Keelevalikus on keele nimi „Eesti“.

Keelevariant saab oma failid, eraldi ARB-faili ja eraldi sõnastiku. Seda ei segata põhikeele hulka.

## Pöördumine

Air pöördub kasutaja poole sina-vormis kõigis sõnedes, nagu Signal ja Element eesti keeles. Pöördumine ei muutu sõnest sõnesse. Lühivormid „sa“, „su“, „sul“ ja „sulle“ on eelistatud, kui rõhk ei ole asesõnal.

Nupud ja menüüpunktid on käskivas kõneviisis: „Kustuta“, „Saada“, „Tühista“. Juhised ja veateated on samuti käskivas kõneviisis: „Proovi uuesti.“, „Sisesta kood“.

Küsimusena sõnastatud kinnitusdialoogi pealkiri algab sõnaga „Kas“ ja kasutab da-tegevusnime: „Kas kustutada sõnum?“, „Kas blokeerida kasutaja {displayName}?“.

## Suurtähed

Kehtib eesti õigekiri, mitte inglise oma. Pealkirjades ja nuppudes on suur algustäht ainult esimesel sõnal ja pärisnimedel, ka siis, kui ingliskeelses mallis on iga sõna suure tähega. Nädalapäevad, kuud ja keelte nimed on väikese algustähega, kui need ei ole lause alguses.

## Kirjavahemärgid

Jutumärgid on „…“, alumine avav U+201E ja ülemine sulgev U+201C. Ingliskeelsed "..." ja “...” asendatakse tõlkes märkidega „...“. Erand on `deleteAccountScreen_confirmationInputLabel`, kus rakendus võrdleb ülakomade vahel olevat sõna 'delete' täpselt.

Sõne lõpeb sama märgiga kui originaal, olgu see punkt, küsimärk, hüüumärk või koolon. Kui originaalis märki ei ole, ei ole seda ka tõlkes. Kolmikpunkt kordab malli kirjapilti. Kui mallis on kolm eraldi punkti „...“, on need ka tõlkes. Kui mallis on üks märk „…“, on see ka tõlkes.

Kõrvallause eraldatakse komaga, ka siis, kui ingliskeelses lauses koma ei ole: „Puuduta, et oma seadmeid vaadata.“, „Kui see on sisse lülitatud, kustutatakse aegunud kontaktitaotlused automaatselt.“

Menüütee eraldaja jääb samaks kui mallis, näiteks „>“ või „→“.

## Arvud ja ühikud

Arv ja ühik eraldatakse tühikuga: „5 MB“, „5 min“. Baidiühikud on B, kB, MB, GB, TB, PB, EB, ZB ja YB. Kilobait kirjutatakse väikese k-ga. Kümnendkoha eraldaja on koma, selle vormindab rakendus ise.

## Mitmus

Eesti keel kasutab CLDR-i kategooriaid `one` ja `other`. `one` hõlmab ainult arvu 1. Arvu 1 järel on nimisõna nimetavas („1 seade“), suuremate arvude järel ainsuse osastavas („5 seadet“). Haru `other` on nõutud, ilma selleta sõne valideerimist ei läbi. Haru `=0` sõnastatakse ilma arvuta, kui see loeb paremini: „Lingitud seadmeid ei ole.“

Kui tekst on kõigis vormides sama, piisab ühest harust `other`.

## Käänded ja nimed

Eesti keeles ei ole grammatilist sugu. Ingliskeelne „they“ või „them“ ühe inimese kohta on „ta“ või „tema“.

Kohatäitjas olevat nime rakendus ei kääna. See puudutab kuvanimesid, grupi- ja vestluse nimesid, seadmete nimesid ja kasutajanimesid. Seepärast:

- Alusena jääb nimi nimetavasse käändesse: „{displayName} nõustus su kontaktitaotlusega.“
- Kui nimi peaks olema muus käändes, saab käändelõpu nime ees olev üldnimi ja nimi jääb muutmata: „kasutajalt {displayName}“, „kasutaja {displayName} kontaktitaotlus“, „grupist {chatName}“.
- Üldnimi on vaikimisi „kasutaja“. Kui ingliskeelne tekst ütleb „contact“, on üldnimi „kontakt“. Grupi nime ees on „grupp“, vestluse nime ees „vestlus“, seadme nime ees „seade“ ja kasutajanime ees „kasutajanimi“.
- Ära kirjuta käändelõppu nime järele, ei sidekriipsuga ega ülakomaga, näiteks „{displayName}-lt“.

## Air

„Air“ käändub nagu iga teine nimisõna, ilma sidekriipsu ja ülakomata: Air, Airi, Airis, Airist, Airile, Airiga. „Air account“ on „Airi konto“, „Air contact“ on „Airi kontakt“, „on Air“ on „Airis“ ja „Join Air“ on „Liitu Airiga“. Tüvi „Air“ jääb igas vormis muutmata ja suure algustähega, nii et valideerija leiab selle üles.

## Põhiterminid

| Termin | Tähendus | Kontekst |
|--------|----------|----------|
| **Air** | Rakenduse nimi | Ei tõlgita, aga käändub: „Airi konto“, „Airis“ |
| **Konto** | Kasutaja konto Airis | „Airi konto“ |
| **Kasutajanimi** | Unikaalne tunnus, mida jagatakse, et teised saaksid ühendust võtta | Ainult ühenduse võtmiseks. Mitmus „kasutajanimed“ |
| **Kuvanimi** | Nimi, mida teised vestlustes näevad | Erineb kasutajanimest. Mitte „kuvatav nimi“ |
| **Vestlus** | Kahe või enama inimese kirjavahetus | Inglise „chat“. Mitte „jutt“ ega „chat“ |
| **Kontakt** | Teine kasutaja, kellele saad kirjutada | „Airi kontakt“ |
| **Liige** | Grupivestluse osaleja | Gruppide kontekstis |
| **Inimene, inimesed** | Inglise „person, people“ | „Lisa inimesi“, „5 inimest“ |
| **Rakendus** | Inglise „app“ | Mitte „äpp“ |

## Sõnumid

| Termin | Tähendus | Kontekst |
|--------|----------|----------|
| **Sõnum** | Vestlusesse saadetud tekst, pilt või fail | |
| **Mustand** | Kirjutatud, aga saatmata sõnum | Vestluste loendis |
| **Manus** | Sõnumiga saadetud fail või pilt | |
| **Muuda** | Juba saadetud sõnumi muutmine | Inglise „edit“ ja „change“ on mõlemad „muuda“. Märge sõnumil „Muudetud“ |
| **Vasta** | Kindlale sõnumile vastamine | |
| **Reageeri** | Emoji lisamine sõnumile | Nupp „Reageeri“, tekstis „reageeris“ |
| **Emoji** | Pildimärk | Mitmus „emojid“, osastav „emojit“ |
| **Grupp** | Mitme liikmega vestlus | Vestlus ise on „grupivestlus“. Mitte „rühm“ |

## Toimingud ja liides

| Termin | Tähendus | Kontekst |
|--------|----------|----------|
| **Kontaktitaotlus** | Taotlus, mis saadetakse kasutajanime või grupi kaudu | Inglise „contact request“. Inglise „connect“ inimeste kohta on „ühendust võtma“ või „ühenduses olema“. Mitte „linkima“ |
| **Lisa** | Kellegi lisamine gruppi või kontaktide hulka | |
| **Eemalda** | Inglise „remove“ | Inimese eemaldamine grupist, aga ka kasutajanime, reaktsiooni ja kasutatud koodide eemaldamine |
| **Kustuta** | Inglise „delete“ | Sisu jäädav kustutamine: sõnum, vestlus, kontakt, konto. Ei ole „remove“ tõlge |
| **Lahku** | Ise grupist lahkumine | Erineb eemaldamisest, mis puudutab teist inimest |
| **Blokeeri** | Kellegi sõnumite vastuvõtmise lõpetamine | |
| **Tühista blokeering** | Blokeeringu lõpetamine | Sama väljend kõigis sõnedes, ka dialoogi tekstis: „Blokeeringu saad igal ajal tühistada.“ |
| **Vaigista** | Vestluse teavituste peatamine | |
| **Tühista vaigistus** | Vaigistuse lõpetamine | |
| **Lingi** | Teisele seadmele kontole juurdepääsu andmine | Ainult seadmete kohta. Tegevus on „linkimine“ |
| **Tühista linkimine** | Lingitud seadmelt juurdepääsu äravõtmine | Nupp „Tühista linkimine“, tekstis „seadme linkimise tühistamine“ |
| **Nõustu** | Kontaktitaotluse vastuvõtmine | Tekstis „nõustus kontaktitaotlusega“ |
| **Keeldu** | Kontaktitaotlusest keeldumine | Saatjale sellest ei teatata |
| **Võta tagasi** | Saadetud kontaktitaotluse tühistamine | |
| **Teata rämpspostist** | Kasutaja märkimine rämpsposti saatjaks | Modereerimine |
| **Puuduta** | Inglise „tap“ | Telefonid ja tahvelarvutid. Mitte „vajuta“ ega „koputa“ |
| **Klõpsa** | Inglise „click“ | Arvutis, `_desktop`-võtmetes |

## Failid ja andmed

| Termin | Tähendus | Kontekst |
|--------|----------|----------|
| **Baidiühikud** | B, kB, MB, GB, TB, PB, EB, ZB, YB | Arv ja ühik tühikuga |
| **Üles laadima** | Faili saatmine serverisse | Tegevus on „üleslaadimine“ |
| **Pildid** | Galeriist pildi valimine | „Kaamera“ uue pildi tegemiseks |

## Olekud ja aeg

| Termin | Tähendus | Kontekst |
|--------|----------|----------|
| **Nüüd** | Just praegu | Väga uue sõnumi ajatempel |
| **{count} min** | Minutit tagasi | Lühike, punktita |
| **Täna**, **Eile** | Tänane ja eilne päev | Kuupäeva eraldajad sõnumite loendis |
| **Saatmine** | Sõnum on teel serverisse | Sõnumi olek |
| **Saatmine ebaõnnestus** | Sõnumit ei õnnestunud saata | Sõnumi olek |
| **Saadetud** | Sõnum jõudis serverisse | Sõnumi olek |
| **Kohale toimetatud** | Sõnum jõudis saaja seadmesse | Sõnumi olek |
| **Loetud** | Saaja luges sõnumi läbi | Sõnumi olek |
| **Lugemiskinnitused** | Seade, mis määrab, kas lugemise olekut jagatakse | |
| **Aegub** | Kontaktitaotluse aegumine | „Aegub 3 päeva pärast“, „Kontaktitaotlus aegus.“ |

## Seaded ja abi

| Termin | Tähendus | Kontekst |
|--------|----------|----------|
| **Profiil ja seaded** | Seadete kuva | |
| **Sina** | Inglise vahekaart „You“ | Ka liikmete ja reaktsioonide loendis |
| **Turvakood** | Kood, mida kaks kontakti võrdlevad, et veenduda, et keegi vestlust pealt ei kuula | |
| **Otspunktkrüpteerimine** | Inglise „end-to-end encryption“ | Omadussõna on „otspunktkrüpteeritud“ |
| **Kutsekood** | Kood, mis on vajalik Airiga liitumiseks | Alati „kutsekood“ |
| **Seadmed** | Inglise rida „Devices“ | |
| **Lingitud seadmed** | Teised seadmed, mis on samasse kontosse sisse logitud | Praegune seade on „See seade“ |
| **QR-kood** | Seadme linkimise kood | Sidekriipsuga |
| **Numbrikood** | Numbritest koosnev kood seadme linkimiseks | |
| **Server** | Host, kus konto asub | |
| **Teavitused** | Inglise „notifications“ | Inglise „push notifications“ on „tõuketeavitused“ |
| **Logid** | Diagnostikalogid | |
| **Abi** | Abi ja tugi | |
| **Võta Airiga ühendust** | Tugimeeskonnaga ühenduse võtmine | |
| **Litsentsid** | Avatud lähtekoodiga komponentide õigusteave | |
| **Versiooniteave** | Rakenduse versiooni tehnilised andmed | |
| **OK** | Inglise nupp „Okay“ | |

## Tõlkijale

- **Air** ei tõlgita, aga see käändub: „Airi konto“, „Airis“, „Airiga“
- **Kasutajanimi** leiab inimesi, **kuvanimi** tuvastab neid vestlustes
- **Eemalda** ja **kustuta** järgivad ingliskeelset verbi. „Remove“ on alati „eemalda“ ja „delete“ alati „kustuta“
- **Ühendust võtma** ja **kontaktitaotlus** käivad inimeste kohta, **lingi** käib seadmete kohta. Neid mõisteid ei segata
- **Blokeeri** ja **tühista blokeering** on kõigis sõnedes samad, ka dialoogi tekstis
- Kui nimi peaks olema muus kui nimetavas käändes, saab käändelõpu nime ees olev üldnimi
- Kui ingliskeelne tekst on muutunud, tõlgi see uuesti praeguse teksti järgi ja ära paranda vana tõlget
