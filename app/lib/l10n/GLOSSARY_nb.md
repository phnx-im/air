# Lokaliseringsordliste (norsk bokmål)

Denne ordlisten fastsetter de sentrale termene i brukergrensesnittet til Air, slik at oversettelsene blir enhetlige.

Malen `app_en.arb` bestemmer hva en tekst sier. Ordlisten bestemmer hvilke termer den sier det med. Hvis malen og ordlisten ikke stemmer overens, si fra om konflikten i stedet for å velge selv.

## Variant

Air bruker norsk bokmål etter rettskrivningen i Bokmålsordboka fra Språkrådet. Koden til oversettelsen er `nb`, og språkvelgeren viser «Norsk bokmål».

En språkvariant får egne filer, en egen ARB-fil og en egen ordliste, og blandes ikke inn i grunnspråket. Nynorsk ville vært `app_nn.arb` med `GLOSSARY_nn.md`.

## Tiltale

Air sier «du» til brukeren i alle tekster, slik Android og Apple gjør. Tiltalen varierer ikke fra tekst til tekst.

Knapper og menyvalg står i imperativ: «Slett», «Send», «Avbryt», «Prøv igjen». Instruksjoner og feilmeldinger står også i imperativ: «Prøv igjen.», «Skriv inn koden».

En bekreftelsestittel som er formulert som et spørsmål, begynner med «Vil du»: «Vil du slette meldingen?», «Vil du blokkere {displayName}?».

Faste formuleringer:

- «Are you sure you want to…» blir «Er du sikker på at du vil …?».
- «Failed to…» og «Couldn't…» blir «Kunne ikke …»: «Kunne ikke sende meldingen. Prøv igjen.»
- Hvis den engelske teksten ikke inneholder «please», legger ikke oversettelsen til «vennligst».

## Store bokstaver

Norsk rettskrivning gjelder, ikke engelsk. I titler og på knapper har bare det første ordet og egennavn stor forbokstav, også når den engelske malen skriver hvert ord med stor forbokstav: «Invitasjonskoder», «Sikkerhetskode». Ukedager, måneder og språknavn skrives med liten forbokstav: «mandag», «engelsk». Dokumenttitler som «Bruksvilkår» har bare stor forbokstav på det første ordet.

## Tegnsetting

Anførselstegn er «…». Inni dem brukes ‘…’. Engelske "..." og “...” blir «...» i oversettelsen. Unntaket er `deleteAccountScreen_confirmationInputLabel`, der ordet 'delete' står uoversatt mellom rette apostrofer fordi appen sammenligner nøyaktig det ordet.

Navnet på en knapp, en fane eller et valg i en setning står i anførselstegn, også når malen ikke har dem: trykk på «Koble til en enhet».

Det står komma etter en leddsetning som innleder setningen: «Hvis du blokkerer personen, får du ikke flere meldinger.» Det står også komma etter en leddsetning inni setningen: «Meldinger som ikke er sendt, lagres på enheten.» Det står ikke komma foran en at-setning eller en nødvendig som-setning.

En tekst slutter med samme tegn som originalen. Det kan være punktum, spørsmålstegn, utropstegn eller kolon. Har originalen ikke noe tegn til slutt, har heller ikke oversettelsen det. Tre prikker følger malen. Har malen tre punktum «...», har også oversettelsen det. Har den tegnet «…», har også oversettelsen det.

Skilletegnet i en menysti er det samme som i malen, for eksempel «>» eller «→».

## Navn og eieform

Appen bøyer ikke navnene i plassholderne. Derfor:

- Det står aldri en genitiv-s eller en apostrof rett etter en plassholder. «{displayName}'s contact request» blir «kontaktforespørselen fra {displayName}».
- Der det gjør teksten tydeligere, står et fellesnavn foran plassholderen: «gruppen {groupName}», «enheten {device}», «brukernavnet {username}».
- «Han» og «hun» avslører kjønnet til en person. Engelsk «they» eller «them» om én person blir «personen», eller setningen gjentar navnet: «til du opphever blokkeringen av personen».

## Sammensatte ord

Et sammensatt ord med et egennavn eller en forkortelse får bindestrek: «Air-konto», «Air-kontakt», «QR-kode». Et sammensatt ord med et uttrykk på flere ord får bindestrek mellom alle ledd: «ende-til-ende-kryptering». Sammensatte ord skrives alltid i ett: «gruppesamtale», ikke «gruppe samtale».

## Tall og enheter

Det står mellomrom mellom tall og enhet: «5 MB». Byteenhetene er B, kB, MB, GB, TB, PB, EB, ZB og YB, med liten k som i Android. Desimaltegnet er komma, og appen formaterer det.

Tidsstempelet for minutter er «{count} min», uten punktum.

## Flertall

Norsk bokmål bruker CLDR-kategoriene `one` og `other`. `one` dekker bare tallet 1. Grenen `other` er påkrevd, og uten den feiler kontrollen. Grenen `=0` formuleres uten tall når det leser bedre: «Ingen tilkoblede enheter.»

Hvis teksten er lik i alle former, holder det med én gren `other`.

## Air

«Air» oversettes ikke og bøyes ikke. Det får ikke genitiv-s: «serverne til Air», ikke «Airs servere». «Air account» er «Air-konto», «Air contact» er «Air-kontakt», «on Air» er «på Air», «Join Air» er «Bli med på Air» og «Contact Air» er «Kontakt Air».

## Grunntermer

| Term | Betydning | Kontekst |
|------|-----------|----------|
| **Air** | Navnet på appen | Oversettes ikke og bøyes ikke |
| **Konto** | En brukers konto på Air | «Air-konto» |
| **Brukernavn** | Et unikt navn man deler slik at andre kan komme i kontakt | Bare for å komme i kontakt. Flertall «brukernavn» |
| **Visningsnavn** | Navnet andre ser i samtaler | Engelsk «display name». Ikke det samme som brukernavnet. Ikke «profilnavn» |
| **Samtale** | En samtale mellom to eller flere personer | Engelsk «chat». «samtalen», flertall «samtaler». Ikke «chat» |
| **Kontakt** | En annen bruker du kan skrive til | «Air-kontakt» |
| **Medlem** | En deltaker i en gruppesamtale | I grupper |
| **Person, personer** | Engelsk «person, people» | «Legg til personer», «5 personer». Også for «they» og «them» om én person |
| **Bruker** | Alle som bruker Air | |
| **App** | Engelsk «app» | «en app», «appen» |

## Meldinger

| Term | Betydning | Kontekst |
|------|-----------|----------|
| **Melding** | En tekst, et bilde eller en fil i en samtale | Ikke «beskjed» |
| **Slett for meg**, **Slett for alle** | Slette en melding bare fra egne enheter eller for alle i samtalen | |
| **Utkast** | En melding som er skrevet, men ikke sendt | I samtalelisten |
| **Vedlegg** | En fil eller et bilde som sendes med en melding | |
| **Rediger** | Endre en melding som allerede er sendt | Merket på meldingen er «Redigert». Engelsk «change» er «endre»: «Endre gruppenavn» |
| **Svar** | Svare på en bestemt melding | |
| **Reager** | Legge til en emoji på en melding | I løpende tekst «reagerte med {emoji}» |
| **Emoji** | Et bildetegn | «en emoji» |
| **Gruppe** | En samtale med flere medlemmer | Selve samtalen er en «gruppesamtale» |

## Handlinger og grensesnitt

| Term | Betydning | Kontekst |
|------|-----------|----------|
| **Kontaktforespørsel** | En forespørsel som sendes via et brukernavn eller en gruppe | Engelsk «contact request». Engelsk «connect» om personer er «komme i kontakt». Aldri «koble til» |
| **Legg til** | Legge noen til i en gruppe eller blant kontaktene | |
| **Fjern** | Engelsk «remove» | En person fra en gruppe, men også brukernavn, reaksjoner og brukte koder |
| **Slett** | Engelsk «delete» | Innhold for godt: melding, samtale, kontakt, konto |
| **Forlat** | Selv gå ut av en gruppe | «Forlat gruppen». Ikke det samme som å fjerne, som gjelder en annen person |
| **Blokker** | Slutte å motta meldinger fra noen | |
| **Opphev blokkering** | Oppheve en blokkering | Samme uttrykk på knapper og i dialogtekst |
| **Demp** | Slå av varsler for en samtale | |
| **Opphev demping** | Angre «Demp» | Alltid dette uttrykket |
| **Koble til** | Gi en annen enhet tilgang til kontoen | Bare om enheter. «Tilkoblede enheter» |
| **Koble fra** | Fjerne tilgangen til en tilkoblet enhet | |
| **Godta** | Ta imot en kontaktforespørsel | |
| **Avslå** | Si nei til en kontaktforespørsel | Avsenderen får ikke beskjed |
| **Trekk tilbake** | Kalle tilbake en sendt kontaktforespørsel | «Trekk tilbake forespørselen» |
| **Rapporter spam** | Melde noen som avsender av uønskede meldinger | Moderering. Ikke «søppelpost», som gjelder e-post |
| **Trykk** | Engelsk «tap» | Telefoner og nettbrett. Også i en tekst om en annen enhet |
| **Klikk** | Engelsk «click» | Datamaskin, i `_desktop`-nøklene |
| **Lenke** | Engelsk «link» til en nettadresse | «Vil du åpne lenken?». Forveksles ikke med «koble til», som bare gjelder enheter |

## Filer og data

| Term | Betydning | Kontekst |
|------|-----------|----------|
| **Byteenheter** | B, kB, MB, GB, TB, PB, EB, ZB, YB | Mellomrom mellom tall og enhet |
| **Laste opp** | Sende en fil til serveren | Substantivet er «opplasting» |
| **Bilder** | Velge et bilde fra bildebiblioteket | «Kamera» for et nytt bilde |

## Status og tid

| Term | Betydning | Kontekst |
|------|-----------|----------|
| **Nå** | Akkurat nå | Tidsstempel for en helt ny melding |
| **{count} min** | Minutter siden | Kort, uten punktum |
| **I dag**, **I går** | Dagen i dag og dagen før | Datoskiller i meldingslisten |
| **Sender** | Meldingen er på vei til serveren | Meldingsstatus |
| **Kunne ikke sendes** | Meldingen ble ikke sendt | Meldingsstatus |
| **Sendt** | Meldingen har kommet fram til serveren | Meldingsstatus |
| **Levert** | Meldingen har kommet fram til mottakerens enhet | Meldingsstatus |
| **Lest** | Mottakeren har sett meldingen | Meldingsstatus |
| **Lesebekreftelser** | Innstillingen som styrer om lesestatus deles | |
| **Utløper** | At en kontaktforespørsel utløper | «Utløper om 3 dager», «Kontaktforespørselen er utløpt.» |

## Innstillinger og hjelp

| Term | Betydning | Kontekst |
|------|-----------|----------|
| **Profil og innstillinger** | Innstillingsskjermen | |
| **Deg** | Engelsk «You», fanen | Også i medlemslisten og ved reaksjoner |
| **Notater til meg selv** | Notatsamtalen mellom dine egne enheter | Engelsk «Notes to self» |
| **Sikkerhetskode** | En kode som to kontakter sammenligner for å sikre at ingen avlytter samtalen | |
| **Ende-til-ende-kryptering** | Engelsk «end-to-end encryption» | Adjektivet er «ende-til-ende-kryptert» |
| **Invitasjonskode** | En kode som trengs for å bli med på Air | Alltid «invitasjonskode» |
| **Enheter** | Engelsk «Devices», raden | |
| **Tilkoblede enheter** | De andre enhetene som er logget inn på samme konto | Den aktuelle enheten er «Denne enheten» |
| **QR-kode** | En kode for å koble til en enhet | |
| **Tallkode** | En kode av sifre for å koble til en enhet | Engelsk «numeric code» |
| **Server** | Datamaskinen kontoen ligger på | |
| **Varsler** | Engelsk «notifications» | «push notifications» er «pushvarsler» |
| **Logger** | Diagnostiske logger | Engelsk «logs» |
| **Hjelp** | Hjelp og støtte | |
| **Kontakt Air** | Kontakt med brukerstøtten | |
| **Lisenser** | Juridisk informasjon om komponenter med åpen kildekode | |
| **Versjonsinformasjon** | Tekniske opplysninger om appversjonen | |
| **OK** | Engelsk «Okay», knappen | |

## Til oversetteren

- **Air** oversettes ikke, bøyes ikke og får ikke genitiv-s
- **Brukernavn** finner personer, **visningsnavn** kjenner dem igjen i samtaler
- **Fjern** og **slett** følger det engelske verbet. «remove» er alltid «fjern», «delete» er alltid «slett»
- **Komme i kontakt** og **kontaktforespørsel** gjelder personer, **koble til** gjelder enheter. Begrepene blandes ikke
- **Blokker** og **opphev blokkering** er de samme i alle tekster, også i dialoger
- Ingen genitiv-s etter en plassholder. Skriv «fra {displayName}»
- Sammensatte ord skrives i ett, med bindestrek etter egennavn og forkortelser
- Hvis den engelske teksten er endret, oversett på nytt fra den gjeldende teksten i stedet for å rette den gamle oversettelsen
