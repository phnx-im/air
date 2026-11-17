# Lokaliseringsordliste (dansk)

Denne ordliste fastlægger de centrale termer i brugerfladen i Air så oversættelserne er ensartede.

Skabelonen `app_en.arb` bestemmer hvad en tekst siger. Ordlisten bestemmer hvilke termer den siger det med. Hvis skabelonen og ordlisten ikke stemmer overens, så gør opmærksom på konflikten i stedet for selv at vælge.

## Variant

Air bruger rigsdansk efter Retskrivningsordbogen fra Dansk Sprognævn. Oversættelsens kode er `da`, og sprogvælgeren viser »Dansk«.

En sprogvariant får sine egne filer, en egen ARB-fil og en egen ordliste, og blandes ikke ind i grundsproget.

## Tiltale

Air siger »du« til brugeren i alle tekster, ligesom Android og Apple. Tiltalen skifter ikke fra tekst til tekst.

Knapper og menupunkter står i bydeform: »Slet«, »Send«, »Annuller«, »Prøv igen«. Instruktioner og fejlmeddelelser står også i bydeform: »Prøv igen.«, »Indtast koden«. Bydeformen skrives uden accent: »Bloker«, »Rediger«, »Accepter«.

En bekræftelsestitel der er formuleret som et spørgsmål, begynder med »Vil du«: »Vil du slette beskeden?«, »Vil du blokere {displayName}?«.

Faste formuleringer:

- »Are you sure you want to…« bliver »Er du sikker på at du vil …?«.
- »Failed to…« og »Couldn't…« bliver »Kunne ikke …«: »Beskeden kunne ikke sendes. Prøv igen.«
- Hvis den engelske tekst ikke indeholder »please«, tilføjer oversættelsen ikke »venligst«.

## Store bogstaver

Dansk retskrivning gælder, ikke engelsk. I titler og på knapper har kun det første ord og egennavne stort begyndelsesbogstav, også når den engelske skabelon skriver hvert ord med stort: »Invitationskoder«, »Sikkerhedskode«. Ugedage, måneder og sprognavne skrives med lille: »mandag«, »engelsk«. Dokumenttitler som »Vilkår for brug« har kun stort begyndelsesbogstav på det første ord.

## Tegnsætning

Anførselstegn er »…«. Inden i dem bruges ›…‹. Engelske "..." og “...” bliver til »...« i oversættelsen. Undtagelsen er `deleteAccountScreen_confirmationInputLabel`, hvor ordet 'delete' står uoversat mellem lige apostroffer fordi appen sammenligner præcis det ord.

Navnet på en knap, en fane eller en indstilling i en sætning står i anførselstegn, også når skabelonen ikke har dem: tryk på »Forbind en enhed«.

Air sætter komma uden startkomma. Der står altså intet komma foran en ledsætning: »Er du sikker på at du vil slette beskeden?«. Der står altid komma efter en ledsætning der indleder sætningen: »Hvis du blokerer personen, modtager du ikke flere beskeder.«

En tekst slutter med samme tegn som originalen. Det kan være punktum, spørgsmålstegn, udråbstegn eller kolon. Har originalen intet tegn til sidst, har oversættelsen heller intet. Udeladelsesprikker følger skabelonen. Har skabelonen tre punktummer »...«, har oversættelsen også det. Har den tegnet »…«, har oversættelsen også det.

Skilletegnet i en menusti er det samme som i skabelonen, for eksempel »>« eller »→«.

## Navne og ejefald

Appen bøjer ikke navnene i pladsholderne. Derfor:

- Der står aldrig et ejefalds-s eller en apostrof lige efter en pladsholder. »{displayName}'s contact request« bliver »kontaktanmodningen fra {displayName}«.
- Hvor det gør teksten tydeligere, står et fællesnavn foran pladsholderen: »gruppen {groupName}«, »enheden {device}«, »brugernavnet {username}«.
- »Han« og »hun« afslører en persons køn. Engelsk »they« eller »them« om én person bliver »personen«, eller sætningen gentager navnet: »medmindre du ophæver blokeringen af personen«.

## Sammensatte ord

Et sammensat ord med et egennavn eller en forkortelse får bindestreg: »Air-konto«, »Air-kontakt«, »QR-kode«. Et sammensat ord med en engelsk ordgruppe får bindestreger mellem alle led: »end-to-end-kryptering«. Sammensatte ord skrives altid i ét: »gruppechat«, ikke »gruppe chat«.

## Tal og enheder

Der står mellemrum mellem tal og enhed: »5 MB«. Byteenhederne er B, kB, MB, GB, TB, PB, EB, ZB og YB, med lille k som i Android. Decimaltegnet er komma, og appen formaterer det.

Tidsstemplet for minutter er »{count} min«, uden punktum.

## Flertal

Dansk bruger CLDR-kategorierne `one` og `other`. `one` dækker kun tallet 1. Grenen `other` er påkrævet, og uden den fejler kontrollen. Grenen `=0` formuleres uden tal når det læser bedre: »Ingen forbundne enheder.«

Hvis teksten er ens i alle former, er én gren `other` nok.

## Air

»Air« oversættes ikke og bøjes ikke. Der kommer intet ejefalds-s på: »serverne hos Air«, ikke »Airs servere«. »Air account« er »Air-konto«, »Air contact« er »Air-kontakt«, »on Air« er »på Air«, »Join Air« er »Tilmeld dig Air«, og »Contact Air« er »Kontakt Air«.

## Grundtermer

| Term | Betydning | Kontekst |
|------|-----------|----------|
| **Air** | Appens navn | Oversættes ikke og bøjes ikke |
| **Konto** | En brugers konto på Air | »Air-konto« |
| **Brugernavn** | Et unikt navn man deler så andre kan komme i kontakt med en | Kun til at komme i kontakt. Flertal »brugernavne« |
| **Visningsnavn** | Det navn andre ser i chats | Engelsk »display name«. Ikke det samme som brugernavnet. Ikke »profilnavn« |
| **Chat** | En samtale mellem to eller flere personer | »en chat«, »chatten«, flertal »chats« |
| **Kontakt** | En anden bruger du kan skrive til | »Air-kontakt« |
| **Medlem** | En deltager i en gruppechat | I grupper |
| **Person, personer** | Engelsk »person, people« | »Tilføj personer«, »5 personer«. Også for »they« og »them« om én person |
| **Bruger** | Enhver der bruger Air | |
| **App** | Engelsk »app« | »en app«, »appen« |

## Beskeder

| Term | Betydning | Kontekst |
|------|-----------|----------|
| **Besked** | En tekst, et billede eller en fil i en chat | Ikke »meddelelse« |
| **Slet for mig**, **Slet for alle** | Slette en besked kun fra egne enheder eller for alle i chatten | |
| **Kladde** | En besked der er skrevet men ikke sendt | I chatlisten |
| **Vedhæftet fil** | En fil eller et billede der sendes med en besked | Flertal »vedhæftede filer« |
| **Rediger** | Ændre en besked der allerede er sendt | Mærkat på beskeden »Redigeret«. Engelsk »change« er »skift«: »Skift gruppenavn« |
| **Svar** | Svare på en bestemt besked | |
| **Reager** | Tilføje en emoji til en besked | I løbende tekst »reagerede med {emoji}« |
| **Emoji** | Et billedtegn | »en emoji« |
| **Gruppe** | En chat med flere medlemmer | Selve chatten er en »gruppechat« |

## Handlinger og brugerflade

| Term | Betydning | Kontekst |
|------|-----------|----------|
| **Kontaktanmodning** | En anmodning der sendes via et brugernavn eller en gruppe | Engelsk »contact request«. Engelsk »connect« om personer er »komme i kontakt«. Aldrig »forbinde« |
| **Tilføj** | Føje nogen til en gruppe eller til kontakter | |
| **Fjern** | Engelsk »remove« | En person fra en gruppe, men også brugernavne, reaktioner og brugte koder |
| **Slet** | Engelsk »delete« | Indhold for altid: besked, chat, kontakt, konto |
| **Forlad** | Selv gå ud af en gruppe | »Forlad gruppe«. Ikke det samme som at fjerne, som handler om en anden person |
| **Bloker** | Holde op med at modtage beskeder fra nogen | |
| **Ophæv blokering** | Ophæve en blokering | Samme udtryk på knapper og i dialogtekst |
| **Slå lyd fra** | Slå notifikationer fra for en chat | |
| **Slå lyd til** | Fortryde »Slå lyd fra« | |
| **Forbind** | Give en anden enhed adgang til kontoen | Kun om enheder. »Forbundne enheder« |
| **Fjern forbindelse** | Fjerne en forbundet enheds adgang | Engelsk »unlink«. Ikke »frakobl« |
| **Accepter** | Tage imod en kontaktanmodning | |
| **Afvis** | Sige nej til en kontaktanmodning | Afsenderen får ikke besked |
| **Træk tilbage** | Tilbagekalde en sendt kontaktanmodning | »Træk anmodning tilbage« |
| **Rapporter spam** | Melde nogen som afsender af uønskede beskeder | Moderering |
| **Tryk** | Engelsk »tap« | Telefoner og tablets. Også i en tekst om en anden enhed |
| **Klik** | Engelsk »click« | Computer, i `_desktop`-nøglerne |
| **Link** | Engelsk »link« til en webadresse | »Vil du åbne linket?«. Forveksles ikke med »forbind«, som kun bruges om enheder |

## Filer og data

| Term | Betydning | Kontekst |
|------|-----------|----------|
| **Byteenheder** | B, kB, MB, GB, TB, PB, EB, ZB, YB | Mellemrum mellem tal og enhed |
| **Uploade** | Sende en fil til serveren | Navneordet er »upload« |
| **Billeder** | Vælge et billede fra billedbiblioteket | »Kamera« til et nyt foto |

## Status og tid

| Term | Betydning | Kontekst |
|------|-----------|----------|
| **Nu** | Lige nu | Tidsstempel for en helt ny besked |
| **{count} min** | Minutter siden | Kort, uden punktum |
| **I dag**, **I går** | Den aktuelle og den forrige dag | Datoskillelinjer i beskedlisten |
| **Sender** | Beskeden er på vej til serveren | Beskedstatus |
| **Kunne ikke sendes** | Beskeden blev ikke sendt | Beskedstatus |
| **Sendt** | Beskeden er nået frem til serveren | Beskedstatus |
| **Leveret** | Beskeden er nået frem til modtagerens enhed | Beskedstatus |
| **Læst** | Modtageren har set beskeden | Beskedstatus |
| **Læsekvitteringer** | Indstillingen der styrer om læsestatus deles | |
| **Udløber** | En kontaktanmodnings udløb | »Udløber om 3 dage«, »Kontaktanmodningen er udløbet.« |

## Indstillinger og hjælp

| Term | Betydning | Kontekst |
|------|-----------|----------|
| **Profil og indstillinger** | Indstillingsskærmen | |
| **Dig** | Engelsk »You«, fanen | Også i medlemslisten og ved reaktioner |
| **Noter til mig selv** | Notechatten mellem dine egne enheder | Engelsk »Notes to self« |
| **Sikkerhedskode** | En kode som to kontakter sammenligner for at sikre at ingen aflytter chatten | |
| **End-to-end-kryptering** | Engelsk »end-to-end encryption« | Tillægsordet er »end-to-end-krypteret« |
| **Invitationskode** | En kode der skal bruges for at tilmelde sig Air | Altid »invitationskode« |
| **Enheder** | Engelsk »Devices«, rækken | |
| **Forbundne enheder** | De andre enheder der er logget ind på samme konto | Den aktuelle enhed er »Denne enhed« |
| **QR-kode** | En kode til at forbinde en enhed | |
| **Talkode** | En kode af tal til at forbinde en enhed | Engelsk »numeric code« |
| **Server** | Den computer kontoen ligger på | |
| **Notifikationer** | Engelsk »notifications« | »push notifications« er »pushnotifikationer« |
| **Logfiler** | Diagnostiske logfiler | Engelsk »logs« |
| **Hjælp** | Hjælp og support | |
| **Kontakt Air** | Kontakt til supportteamet | |
| **Licenser** | Juridiske oplysninger om open source-komponenter | |
| **Versionsoplysninger** | Tekniske oplysninger om appens version | |
| **OK** | Engelsk »Okay«, knappen | |

## Til oversætteren

- **Air** oversættes ikke, bøjes ikke og får intet ejefalds-s
- **Brugernavn** finder personer, **visningsnavn** genkender dem i chats
- **Fjern** og **slet** følger det engelske verbum. »remove« er altid »fjern«, »delete« er altid »slet«
- **Komme i kontakt** og **kontaktanmodning** handler om personer, **forbind** handler om enheder. Begreberne blandes ikke
- **Bloker** og **ophæv blokering** er de samme i alle tekster, også i dialoger
- Intet ejefalds-s efter en pladsholder. Skriv »fra {displayName}«
- Komma uden startkomma, men altid komma efter en indledende ledsætning
- Hvis den engelske tekst er ændret, så oversæt igen ud fra den nuværende tekst i stedet for at rette den gamle oversættelse
