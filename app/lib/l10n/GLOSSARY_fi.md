# Lokalisoinnin sanasto (suomi)

Tämä sanasto vahvistaa Airin käyttöliittymän keskeiset termit, jotta käännökset pysyvät yhtenäisinä.

Malli `app_en.arb` määrää, mitä teksti sanoo. Sanasto määrää, millä termeillä se sen sanoo. Jos malli ja sanasto ovat ristiriidassa, ilmoita ristiriidasta äläkä valitse itse.

## Kielimuoto

Käännös on suomen yleiskieltä Kotimaisten kielten keskuksen ohjeiden mukaan. Käännöksen koodi on `fi`, ja kielivalitsimessa näkyy ”Suomi”.

Kielivariantti saa omat tiedostonsa, oman ARB-tiedoston ja oman sanaston, eikä sitä sekoiteta peruskieleen.

## Puhuttelu

Air puhuttelee käyttäjää sinä-muodossa kaikissa teksteissä, kuten Android ja Apple. Puhuttelu ei vaihtele tekstistä toiseen. Pronomini ”sinä” jätetään pois, kun verbin pääte jo kertoo persoonan: ”Poistit viestin.”, ”Haluatko varmasti poistaa viestin?”. Omistusliite riittää: ”yhteyspyyntösi”, ei ”sinun yhteyspyyntösi”.

Painikkeet ja valikon kohdat ovat yksikön 2. persoonan imperatiivissa: ”Poista”, ”Lähetä”, ”Peruuta”, ”Yritä uudelleen”. Myös ohjeet ja virheilmoitukset ovat imperatiivissa: ”Yritä uudelleen.”, ”Anna koodi”.

Kysymyksenä muotoiltu vahvistusotsikko on passiivin kysymysmuoto: ”Poistetaanko viesti?”, ”Estetäänkö käyttäjä {displayName}?”. Objekti on silloin perusmuodossa.

Vakiomuotoilut:

- ”Are you sure you want to…” on ”Haluatko varmasti …?”.
- ”Failed to…” ja ”Couldn't…” on ”… epäonnistui”: ”Viestin lähettäminen epäonnistui. Yritä uudelleen.”
- Käynnissä oleva toiminto on passiivissa: ”Lähetetään…”, ”Yhdistetään…”.
- Jos englanninkielisessä tekstissä ei ole sanaa ”please”, käännökseen ei lisätä kohteliaisuusfraasia.

Suomessa ei ole kieliopillista sukua. Englannin yhtä ihmistä tarkoittava ”they” tai ”them” on ”hän” tai ”häntä”.

## Sijamuodot ja nimet

Sovellus ei taivuta paikkamerkkien nimiä. Tämä koskee näyttönimiä, ryhmien ja keskustelujen nimiä, laitteiden nimiä ja käyttäjänimiä. Siksi:

- Subjektina nimi on perusmuodossa: ”{displayName} hyväksyi yhteyspyyntösi.”
- Kun nimen pitäisi olla muussa sijamuodossa, sijapäätteen saa nimen edessä oleva yleisnimi, ja nimi jää perusmuotoon: ”käyttäjän {displayName}”, ”käyttäjälle {displayName}”, ”käyttäjältä {displayName}”, ”ryhmässä {groupName}”, ”ryhmästä {groupName}”, ”laitteen {device}”, ”käyttäjänimen {username}”.
- Yleisnimi on oletuksena ”käyttäjä”. Kun englanninkielinen teksti sanoo ”contact”, yleisnimi on ”yhteystieto”. Ryhmän nimen edessä on ”ryhmä”, keskustelun nimen edessä ”keskustelu”, laitteen nimen edessä ”laite” ja käyttäjänimen edessä ”käyttäjänimi”.
- Genetiivimääre on pääsanan edessä: ”käyttäjän {displayName} yhteyspyyntö”.
- Paikkamerkkiin ei liitetä päätettä, ei suoraan eikä kaksoispisteen avulla. Ei siis ”{displayName}lle” eikä ”{displayName}:lle”. Kaksoispiste ennen päätettä kuuluu lyhenteille ja numeroille, ei nimille.
- Lukujen, päivämäärien ja kellonaikojen paikkamerkit eivät myöskään saa päätettä. Sijan saa niiden perässä oleva sana: ”{count} päivän kuluttua”, ”{date} klo {time}”.

## Isot kirjaimet

Suomen oikeinkirjoitus pätee, ei englannin. Otsikoissa ja painikkeissa iso alkukirjain on vain ensimmäisessä sanassa ja erisnimissä, vaikka englanninkielinen malli kirjoittaisi jokaisen sanan isolla: ”Kutsukoodit”, ”Turvakoodi”. Viikonpäivät, kuukaudet ja kielten nimet kirjoitetaan pienellä: ”maanantai”, ”englanti”. Asiakirjan nimessä, kuten ”Käyttöehdot”, vain ensimmäinen sana alkaa isolla.

## Välimerkit

Lainausmerkit ovat ”…”, eli molemmat merkit ovat samanlaisia loppulainausmerkkejä. Lainauksen sisällä käytetään merkkejä ’…’. Englannin "..." ja “...” muuttuvat käännöksessä muotoon ”...”. Poikkeus on `deleteAccountScreen_confirmationInputLabel`, jossa sana 'delete' jää kääntämättä suorien heittomerkkien väliin, koska sovellus vertaa juuri tätä sanaa.

Painikkeen, välilehden tai valinnan nimi lauseen sisällä on lainausmerkeissä, vaikka mallissa ei olisi lainausmerkkejä: napauta ”Linkitä laite”.

Sivulauseen edessä on aina pilkku, myös kun englanninkielisessä lauseessa ei ole: ”Varmista, että koodi on oikein.” Sama koskee sanoja ”jos”, ”kun”, ”koska” ja ”joka”.

Teksti päättyy samaan merkkiin kuin alkuteksti. Se voi olla piste, kysymysmerkki, huutomerkki tai kaksoispiste. Jos alkutekstin lopussa ei ole merkkiä, käännöksessäkään ei ole. Kolme pistettä noudattaa mallin kirjoitusasua. Jos mallissa on kolme erillistä pistettä ”...”, niin on käännöksessäkin. Jos mallissa on yksi merkki ”…”, niin on käännöksessäkin.

Valikkopolun erotin pysyy samana kuin mallissa, esimerkiksi ”>” tai ”→”.

## Yhdyssanat

Erisnimen tai lyhenteen kanssa muodostettu yhdyssana saa yhdysmerkin: ”Air-tili”, ”Air-yhteystieto”, ”QR-koodi”. Jos alkuosa on monisanainen, yhdysmerkin edessä on välilyönti: ”päästä päähän -salaus”. Yhdyssanat kirjoitetaan yhteen: ”ryhmäkeskustelu”, ei ”ryhmä keskustelu”.

## Luvut ja yksiköt

Luvun ja yksikön välissä on välilyönti: ”5 Mt”. Tavuyksiköt ovat t, kt, Mt, Gt, Tt, Pt, Et, Zt ja Yt, kuten Androidissa. Englannin ”B” on siis ”t”. Desimaalierotin on pilkku, ja sovellus muotoilee sen.

Minuuttien aikaleima on ”{count} min” ilman pistettä.

## Monikko

Suomi käyttää CLDR-luokkia `one` ja `other`. Luokka `one` kattaa vain luvun 1. Luvun 1 jälkeen substantiivi on perusmuodossa (”1 laite”), suurempien lukujen jälkeen yksikön partitiivissa (”5 laitetta”). Haara `other` on pakollinen, ja ilman sitä tarkistus epäonnistuu. Haara `=0` muotoillaan ilman lukua, jos se luetaan paremmin: ”Ei linkitettyjä laitteita.”

Jos teksti on kaikissa muodoissa sama, yksi `other`-haara riittää.

## Air

”Air” ei käänny, mutta se taipuu kuten muutkin konsonanttiin päättyvät vieraat nimet, sidevokaalin i avulla: Air, Airin, Airia, Airissa, Airista, Airiin, Airilla, Airilta, Airille, Airina, Airiksi. Vartalo ”Air” pysyy kaikissa muodoissa samana ja isolla alkukirjaimella, eikä päätettä eroteta kaksoispisteellä tai yhdysmerkillä. Näin tarkistus löytää nimen. Yhdyssanoissa käytetään yhdysmerkkiä: ”Air-tili”, ”Air-yhteystieto”. ”on Air” on ”Airissa”, ”Join Air” on ”Liity Airiin”, ”Contact Air” on ”Ota yhteyttä Airiin” ja ”Update Air” on ”Päivitä Air”.

## Keskeiset termit

| Termi | Merkitys | Konteksti |
|-------|----------|-----------|
| **Air** | Sovelluksen nimi | Ei käänny, mutta taipuu: ”Airissa”, ”Airiin” |
| **Tili** | Käyttäjän tili Airissa | ”Air-tili” |
| **Käyttäjänimi** | Yksilöllinen nimi, jonka jakamalla muut voivat ottaa yhteyttä | Vain yhteydenottoon. Monikko ”käyttäjänimet” |
| **Näyttönimi** | Nimi, jonka muut näkevät keskusteluissa | Engl. ”display name”. Eri asia kuin käyttäjänimi. Ei ”profiilin nimi” |
| **Keskustelu** | Kahden tai useamman ihmisen viestinvaihto | Engl. ”chat”. Ei ”chatti” |
| **Yhteystieto** | Toinen käyttäjä, jolle voit kirjoittaa | Engl. ”contact”. ”Air-yhteystieto” |
| **Jäsen** | Ryhmäkeskustelun osallistuja | Ryhmissä |
| **Henkilö, henkilöt** | Engl. ”person, people” | ”Lisää henkilöitä”, ”5 henkilöä” |
| **Käyttäjä** | Kuka tahansa, joka käyttää Airia | Oletusyleisnimi paikkamerkin edessä |
| **Sovellus** | Engl. ”app” | Ei ”äppi” |

## Viestit

| Termi | Merkitys | Konteksti |
|-------|----------|-----------|
| **Viesti** | Keskusteluun lähetetty teksti, kuva tai tiedosto | |
| **Poista minulta**, **Poista kaikilta** | Viestin poistaminen vain omilta laitteilta tai kaikilta keskustelun jäseniltä | |
| **Luonnos** | Kirjoitettu mutta lähettämätön viesti | Keskusteluluettelossa |
| **Liite** | Viestin mukana lähetetty tiedosto tai kuva | |
| **Muokkaa** | Jo lähetetyn viestin muuttaminen | Viestin merkintä on ”Muokattu”. Engl. ”change” on ”Vaihda”: ”Vaihda ryhmän nimi” |
| **Vastaa** | Vastaaminen tiettyyn viestiin | |
| **Reagoi** | Emojin lisääminen viestiin | Tekstissä ”reagoi” |
| **Emoji** | Kuvamerkki | Monikko ”emojit” |
| **Ryhmä** | Usean jäsenen keskustelu | Keskustelu itse on ”ryhmäkeskustelu” |

## Toiminnot ja käyttöliittymä

| Termi | Merkitys | Konteksti |
|-------|----------|-----------|
| **Yhteyspyyntö** | Käyttäjänimen tai ryhmän kautta lähetetty pyyntö | Engl. ”contact request”. Engl. ”connect” ihmisistä on ”ottaa yhteyttä”. Ei koskaan ”linkittää” |
| **Lisää** | Jonkun lisääminen ryhmään tai yhteystietoihin | |
| **Poista** | Engl. ”remove” ja ”delete” | Sama verbi molemmille. Ero näkyy objektista: ”Poista ryhmästä”, ”Poista viesti” |
| **Poistu** | Ryhmästä lähteminen omasta aloitteesta | ”Poistu ryhmästä”. Eri asia kuin poistaminen, joka koskee toista henkilöä |
| **Estä** | Viestien vastaanottamisen lopettaminen joltakulta | |
| **Poista esto** | Eston kumoaminen | Sama ilmaus painikkeissa ja valintaikkunoiden tekstissä |
| **Mykistä** | Keskustelun ilmoitusten keskeyttäminen | |
| **Poista mykistys** | Mykistyksen kumoaminen | Aina tämä ilmaus |
| **Linkitä** | Toiselle laitteelle annettava pääsy tiliin | Vain laitteista. ”Linkitetyt laitteet”, ”linkitys” |
| **Poista linkitys** | Linkitetyn laitteen pääsyn peruminen | |
| **Hyväksy** | Yhteyspyynnön hyväksyminen | |
| **Hylkää** | Yhteyspyynnön hylkääminen | Lähettäjälle ei ilmoiteta |
| **Vedä takaisin** | Lähetetyn yhteyspyynnön peruminen | ”Vedä pyyntö takaisin”. Ei ”Peruuta”, joka on peruutuspainike |
| **Ilmoita roskapostista** | Jonkun merkitseminen ei-toivottujen viestien lähettäjäksi | Moderointi |
| **Napauta** | Engl. ”tap” | Puhelimet ja tabletit. Myös tekstissä, joka kuvaa toista laitetta |
| **Napsauta** | Engl. ”click” | Tietokone, `_desktop`-avaimissa |
| **Linkki** | Engl. ”link” verkko-osoitteeseen | ”Avataanko linkki?”. Ei sekoitu laitteen linkittämiseen, koska ”linkki” on substantiivi |

## Tiedostot ja tiedot

| Termi | Merkitys | Konteksti |
|-------|----------|-----------|
| **Tavuyksiköt** | t, kt, Mt, Gt, Tt, Pt, Et, Zt, Yt | Välilyönti luvun ja yksikön välissä |
| **Ladata palvelimelle** | Tiedoston lähettäminen palvelimelle | Engl. ”upload”. Substantiivi on ”lataus palvelimelle”, ei pelkkä ”lataus”, joka tarkoittaa myös lataamista laitteelle |
| **Kuvat** | Kuvan valitseminen kuvakirjastosta | ”Kamera” uuden kuvan ottamiseen |

## Tila ja aika

| Termi | Merkitys | Konteksti |
|-------|----------|-----------|
| **Nyt** | Juuri nyt | Hyvin uuden viestin aikaleima |
| **{count} min** | Minuuttia sitten | Lyhyt, ilman pistettä |
| **Tänään**, **Eilen** | Kuluva ja edellinen päivä | Päivämäärien erottimet viestiluettelossa |
| **Lähetetään** | Viesti on matkalla palvelimelle | Viestin tila |
| **Lähetys epäonnistui** | Viestiä ei voitu lähettää | Viestin tila |
| **Lähetetty** | Viesti on saapunut palvelimelle | Viestin tila |
| **Toimitettu** | Viesti on saapunut vastaanottajan laitteeseen | Viestin tila |
| **Luettu** | Vastaanottaja on nähnyt viestin | Viestin tila |
| **Lukukuittaukset** | Asetus, joka määrää, jaetaanko lukutila | |
| **Vanhenee** | Yhteyspyynnön vanheneminen | ”Vanhenee 3 päivän kuluttua”, ”Yhteyspyyntö vanheni.” |

## Asetukset ja ohje

| Termi | Merkitys | Konteksti |
|-------|----------|-----------|
| **Profiili ja asetukset** | Asetusnäkymä | |
| **Sinä** | Engl. ”You”, välilehti | Myös jäsenluettelossa ja reaktioissa |
| **Muistiinpanot itselle** | Omien laitteiden välinen muistiinpanokeskustelu | Engl. ”Notes to self” |
| **Turvakoodi** | Koodi, jota kaksi yhteystietoa vertaa varmistaakseen, ettei kukaan kuuntele keskustelua | |
| **Päästä päähän -salaus** | Engl. ”end-to-end encryption” | Adjektiivi on ”päästä päähän salattu” |
| **Kutsukoodi** | Koodi, joka tarvitaan Airiin liittymiseen | Aina ”kutsukoodi” |
| **Laitteet** | Engl. ”Devices”, rivi | |
| **Linkitetyt laitteet** | Muut samalle tilille kirjautuneet laitteet | Nykyinen laite on ”Tämä laite” |
| **QR-koodi** | Koodi laitteen linkittämiseen | |
| **Numerokoodi** | Pelkistä numeroista koostuva koodi laitteen linkittämiseen | |
| **Palvelin** | Tietokone, jolla tili sijaitsee | Engl. ”server” |
| **Ilmoitukset** | Engl. ”notifications” | ”push notifications” on ”push-ilmoitukset” |
| **Lokit** | Diagnostiikkalokit | Engl. ”logs” |
| **Ohje** | Ohjeet ja tuki | |
| **Ota yhteyttä Airiin** | Yhteydenotto tukitiimiin | |
| **Lisenssit** | Avoimen lähdekoodin osien oikeudelliset tiedot | |
| **Versiotiedot** | Sovelluksen version tekniset tiedot | |
| **OK** | Engl. ”Okay”, painike | |

## Kääntäjälle

- **Air** ei käänny, mutta taipuu: ”Airissa”, ”Airiin”, ”Airin”
- **Käyttäjänimellä** löydetään ihmisiä, **näyttönimellä** heidät tunnistetaan keskusteluissa
- **Poista** tarkoittaa sekä ”remove” että ”delete”. Ero näkyy objektista tai sanasta ”ryhmästä”
- **Ottaa yhteyttä** ja **yhteyspyyntö** koskevat ihmisiä, **linkitä** koskee laitteita. Käsitteitä ei sekoiteta
- **Estä** ja **poista esto** ovat samat kaikissa teksteissä, myös valintaikkunoissa
- Paikkamerkkiin ei liitetä päätettä. Sijan saa yleisnimi: ”käyttäjän {displayName}”
- Vahvistusotsikot ovat passiivin kysymyksiä: ”Poistetaanko viesti?”
- Jos englanninkielinen teksti on muuttunut, käännä se uudelleen nykyisestä tekstistä äläkä korjaa vanhaa käännöstä
