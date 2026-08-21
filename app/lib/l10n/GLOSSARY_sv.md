# Lokaliseringsordlista (Svenska)

Den här ordlistan definierar nyckeltermer som används i Airs användargränssnitt för att säkerställa konsekventa översättningar.

## Variant

Endast rikssvenska används. Finlandssvenska särdrag gäller inte.

En språkvariant får egna filer, en egen ARB-fil och en egen ordlista, i stället för att blandas in i basspråket. Finlandssvenska skulle alltså vara `app_sv_FI.arb` med `GLOSSARY_sv_FI.md`.

## Tilltal

Svenska använder du-form i hela appen, i linje med svenska UI-konventioner. Det gäller alla strängar och varierar inte från sträng till sträng.

## Versaler

Svensk ortografi gäller, inte engelsk. Svenska använder färre versaler än engelska, så versalerna i en engelsk rubrik följer inte med.

## Grundläggande apptermer

| Term | Definition | Kontext |
|------|-----------|---------|
| **Air** | Namnet på appen | Behåll alltid "Air", översätt inte |
| **Konto** | En användares Air-konto | Visas som "Air-konto". "Air" finns kvar även i sammansättningar |
| **Användarnamn** | En unik identifierare som användare kan dela för att ansluta till andra | Används bara för anslutningar |
| **Visningsnamn** | Namnet som visas i chattar och är synligt för andra användare | Skiljer sig från användarnamn. Det är vad andra ser när du skickar meddelanden |
| **Chatt** | En chatttråd mellan två eller flera personer | Mellan två personer eller i en grupp |
| **Kontakt** | En annan användare som du kan skicka meddelanden till | Visas som "Air-kontakt" |
| **Medlem** | En deltagare i en gruppchatt | Används i gruppchatt-sammanhang |

## Meddelandetermer

| Term | Definition | Kontext |
|------|-----------|---------|
| **Meddelande** | En text, bild eller fil som skickas i en chatt | |
| **Utkast** | Ett osänt meddelande som har skrivits men inte skickats ännu | Visas i chattlistan |
| **Bilaga** | En fil eller bild som skickas med ett meddelande | |
| **Skriv** | Att skriva ett nytt meddelande | |
| **Redigera** | Att ändra ett meddelande som redan har skickats | |
| **Svara** | Att svara på ett specifikt meddelande i en chatt | |
| **Reagera** | Att fästa en emoji på ett meddelande | Åtgärd i meddelandets kontextmeny |
| **Emoji** | Ett enskilt bildtecken | Används i reaktionsväljaren |
| **Grupp** | En chatt med flera deltagare | |

## Åtgärder & gränssnitt

| Term | Definition | Kontext |
|------|-----------|---------|
| **Anslut** | Att lägga till någon som kontakt med hjälp av deras användarnamn | Första åtgärden för att börja chatta med någon ny. Inte samma sak som "länka" |
| **Lägg till** | Att inkludera någon i en grupp eller lägga till dem i kontakter | |
| **Ta bort** | Att ta bort någon från en gruppchatt | Använd "ta bort" när engelskan säger "remove" |
| **Radera** | Att permanent ta bort innehåll (meddelanden, filer osv.) | Använd "radera" när engelskan säger "delete" |
| **Lämna** | Att själv gå ur en grupp | Till skillnad från "ta bort", som gäller någon annan |
| **Blockera** | Att sluta ta emot meddelanden från någon | |
| **Avblockera** | Att upphäva en blockering | Ett enda ord, även i dialogtext |
| **Tysta** | Att stänga av aviseringar för en chatt | |
| **Sluta tysta** | Att upphäva en tystning | |
| **Länka** | Att ge en annan enhet åtkomst till kontot | Gäller enheter, inte kontakter. Inte samma sak som "anslut" |
| **Avlänka** | Att dra in åtkomsten för en länkad enhet | |
| **Rapportera spam** | Att markera en användare eller ett meddelande som oönskat/spam | Modereringsfunktion |

## Fil- och datatermer

| Term | Definition | Kontext |
|------|-----------|---------|
| **Byte-enheter** | Mätningar av filstorlek (B, kB, MB, GB osv.) | För bilagor, använd svenska/standardiserade enheter där det passar |
| **Bilagestorlek** | Filstorleken på uppladdat innehåll | |
| **Ladda upp** | Att skicka en fil eller bild | |

## Status & tid

| Term | Definition | Kontext |
|------|-----------|---------|
| **Nu** | Den aktuella tidpunkten | Tidsstämpel för mycket färska meddelanden |
| **Igår** | Dagen före idag | Tidsstämpel för meddelanden från igår |
| **Skickar** | Meddelandet är på väg till servern | Statusindikator för meddelanden |
| **Kunde inte skicka** | Meddelandet kunde inte skickas | Statusindikator för meddelanden |
| **Skickat** | Meddelandet har skickats till servern | Statusindikator för meddelanden |
| **Levererat** | Meddelandet har levererats till mottagarens enhet | Statusindikator för meddelanden |
| **Läst** | Meddelandet har lästs av mottagaren | Statusindikator för meddelanden |
| **Läskvitton** | Inställningen som styr om lässtatus delas | Reglage i inställningarna |
| **Redigerad** | Anger att ett meddelande har ändrats efter att det skickades | Visas bredvid ändrade meddelanden |

## Inställningar & hjälp

| Term | Definition | Kontext |
|------|-----------|---------|
| **Inställningar** | Appens konfigurationsalternativ | Skärmen heter "Profil och inställningar" |
| **Profil** | Användarens personliga information och inställningar | |
| **Säkerhetskod** | Kod som två kontakter jämför för att verifiera sin chatt | Rad på kontaktens profil och en egen vy |
| **Inbjudningskod** | Kod som behövs för att gå med i Air | Alltid "inbjudningskod" |
| **Länkade enheter** | De andra enheterna som är inloggade på kontot | Avsnitt i inställningarna |
| **Server** | Värden där ett konto ligger | Väljs vid registrering och vid länkning |
| **Hjälp** | Support- och hjälpsida | |
| **Kontakta Air** | Kontaktalternativ för support | |
| **Licenser** | Juridisk information om open source-komponenter | |
| **Versionsinformation** | Tekniska detaljer om appens version | |

## Anteckningar för översättare

- **Air** ska aldrig översättas, det är produktnamnet, och det står kvar i sammansättningar som "Air-konto"
- **Användarnamn** vs **Visningsnamn**: användarnamn används för att hitta personer, visningsnamn för identifiering i chattar
- **Ta bort** vs **Radera**: den engelska mallen väljer verbet. Översättningen följer det engelska verbet i stället för att själv omklassificera objektet
- **Anslut** vs **Länka**: anslut gäller kontakter, länka gäller enheter. Orden hålls åtskilda
- **Blockera** och **Avblockera** har ett enda ord var, även i dialogtext
- Anpassa meddelandeterminologi efter svensk kontext
- **Byte-enheter**: använd svenska/standardiserade enheter (B, kB, MB, GB) där det passar
