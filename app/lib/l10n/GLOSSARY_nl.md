# Lokalisatiewoordenlijst (Nederlands)

Deze woordenlijst legt de kerntermen van de gebruikersinterface van Air vast, zodat de vertalingen consistent blijven.

Het sjabloon `app_en.arb` bepaalt wat een tekst zegt. Deze woordenlijst bepaalt met welke termen dat gebeurt. Als het sjabloon en de woordenlijst elkaar tegenspreken, meld je het conflict in plaats van zelf een keuze te maken.

## Variant

Air gebruikt standaardnederlands dat in Nederland en Vlaanderen even goed leest. De spelling volgt de Woordenlijst Nederlandse Taal (het Groene Boekje). De code van de vertaling is `nl` en de taalkiezer toont ‘Nederlands’.

Een taalvariant krijgt eigen bestanden, een eigen ARB-bestand en een eigen woordenlijst, en wordt niet in de basistaal gemengd. Belgisch-Nederlands zou `app_nl_BE.arb` met `GLOSSARY_nl_BE.md` worden.

## Aanspreekvorm

Air spreekt de gebruiker in alle teksten aan met ‘je’, net als Android en Apple. De aanspreekvorm verandert niet per tekst. Het bezittelijk voornaamwoord is ‘je’ (‘je account’). ‘Jij’ en ‘jouw’ staan er alleen als de nadruk op de persoon ligt.

Knoppen en menu-items staan in de infinitief: ‘Verwijderen’, ‘Verzenden’, ‘Annuleren’, ‘Opnieuw proberen’. Zo doet Android het ook. Instructies en foutmeldingen staan in de gebiedende wijs: ‘Probeer het opnieuw.’, ‘Voer de code in’.

Een bevestigingstitel die een vraag is, staat in de infinitief met een vraagteken: ‘Bericht verwijderen?’, ‘{displayName} blokkeren?’.

Vaste formuleringen:

- ‘Are you sure you want to…’ wordt ‘Weet je zeker dat je … wilt …?’.
- ‘Failed to…’ en ‘Couldn't…’ worden ‘Kan … niet …’: ‘Kan het bericht niet verzenden. Probeer het opnieuw.’ Een korte status wordt ‘… mislukt’: ‘Verzenden mislukt’.
- Als de Engelse tekst geen ‘please’ bevat, voegt de vertaling geen ‘alsjeblieft’ toe.

## Hoofdletters

De Nederlandse spelling geldt, niet de Engelse. In titels en knoppen krijgen alleen het eerste woord en eigennamen een hoofdletter, ook als het Engelse sjabloon elk woord met een hoofdletter schrijft: ‘Uitnodigingscodes’, ‘Veiligheidscode’. Dagen en maanden krijgen een kleine letter, talen een hoofdletter: ‘maandag’, ‘Engels’. Documenttitels zoals ‘Gebruiksvoorwaarden’ krijgen alleen een hoofdletter op het eerste woord.

## Leestekens

Aanhalingstekens zijn enkel, ‘…’. Binnen enkele aanhalingstekens staan dubbele, “…”. De Engelse "..." en “...” worden in de vertaling ‘...’. De uitzondering is `deleteAccountScreen_confirmationInputLabel`, waar het woord 'delete' onvertaald tussen rechte apostrofs blijft, omdat de app precies dat woord vergelijkt.

De naam van een knop, tabblad of optie in een zin staat tussen aanhalingstekens, ook als het sjabloon die niet heeft: tik op ‘Apparaat koppelen’.

Een tekst eindigt op hetzelfde leesteken als het origineel: een punt, vraagteken, uitroepteken of dubbele punt. Heeft het origineel geen leesteken aan het eind, dan heeft de vertaling er ook geen. Het beletselteken volgt het sjabloon. Staan er in het sjabloon drie losse punten ‘...’, dan ook in de vertaling. Staat er het ene teken ‘…’, dan ook in de vertaling.

Het scheidingsteken in een menupad blijft hetzelfde als in het sjabloon, bijvoorbeeld ‘>’ of ‘→’.

## Namen en bezit

De app verbuigt de namen in placeholders niet en weet niet of een naam op een klinker eindigt. Daarom:

- Er komt nooit een ‘s’ of ‘'s’ direct achter een placeholder. ‘{displayName}'s contact request’ wordt ‘het contactverzoek van {displayName}’.
- Waar het duidelijker is, staat een algemeen zelfstandig naamwoord voor de placeholder: ‘de groep {groupName}’, ‘het apparaat {device}’, ‘de gebruikersnaam {username}’.
- ‘Hij’ en ‘zij’ verraden het geslacht van een persoon. Het Engelse ‘they’ of ‘them’ voor één persoon wordt ‘deze persoon’, of de zin herhaalt de naam: ‘tot je deze persoon deblokkeert’.

## Samenstellingen

Een samenstelling met een eigennaam of een afkorting krijgt een koppelteken: ‘Air-account’, ‘Air-contact’, ‘QR-code’. Een samenstelling met een Engelse woordgroep krijgt koppeltekens tussen alle delen: ‘end-to-end-versleuteling’. Een samenstelling wordt nooit los geschreven: ‘groepschat’, niet ‘groep chat’.

## Getallen en eenheden

Tussen getal en eenheid staat een spatie: ‘5 MB’. De byte-eenheden zijn B, kB, MB, GB, TB, PB, EB, ZB en YB, met een kleine k zoals in Android. Het decimaalteken is een komma en de app maakt het op.

De tijdstempel voor minuten is ‘{count} min’, zonder punt.

## Meervoud

Het Nederlands gebruikt de CLDR-categorieën `one` en `other`. `one` dekt alleen het getal 1. De tak `other` is verplicht, zonder die tak faalt de controle. De tak `=0` wordt zonder getal geformuleerd als dat beter leest: ‘Geen gekoppelde apparaten.’

Als de tekst in alle vormen gelijk is, volstaat één tak `other`.

## Air

‘Air’ wordt niet vertaald en niet verbogen. Er komt geen ‘s’ achter, ook niet als bezit: ‘de servers van Air’. ‘Air account’ is ‘Air-account’, ‘Air contact’ is ‘Air-contact’, ‘on Air’ is ‘op Air’, ‘Join Air’ is ‘Lid worden van Air’ en ‘Contact Air’ is ‘Contact opnemen met Air’.

## Kerntermen

| Term | Betekenis | Context |
|------|-----------|---------|
| **Air** | De naam van de app | Niet vertalen en niet verbuigen |
| **Account** | Het account van een gebruiker op Air | ‘het Air-account’ |
| **Gebruikersnaam** | Unieke naam die je deelt zodat anderen contact met je kunnen maken | Alleen om contact te maken. Meervoud ‘gebruikersnamen’ |
| **Weergavenaam** | De naam die anderen in chats zien | Engels ‘display name’. Iets anders dan de gebruikersnaam. Niet ‘profielnaam’ of ‘schermnaam’ |
| **Chat** | Een gesprek tussen twee of meer mensen | ‘de chat’, meervoud ‘chats’. Niet ‘gesprek’ |
| **Contact** | Een andere gebruiker aan wie je kunt schrijven | ‘het Air-contact’, meervoud ‘contacten’ |
| **Lid** | Deelnemer aan een groepschat | In groepen. Meervoud ‘leden’ |
| **Persoon, mensen** | Engels ‘person, people’ | ‘people’ zonder getal is ‘mensen’: ‘Mensen toevoegen’. Na een getal staat ‘personen’: ‘5 personen’. Ook voor ‘they’ en ‘them’ voor één persoon |
| **Gebruiker** | Iedereen die Air gebruikt | |
| **App** | Engels ‘app’ | ‘de app’. Niet ‘applicatie’ |

## Berichten

| Term | Betekenis | Context |
|------|-----------|---------|
| **Bericht** | Een tekst, afbeelding of bestand in een chat | |
| **Verwijderen voor mij**, **Verwijderen voor iedereen** | Een bericht alleen van je eigen apparaten of voor iedereen in de chat verwijderen | |
| **Concept** | Een getypt bericht dat nog niet verzonden is | In de chatlijst |
| **Bijlage** | Een bestand of afbeelding bij een bericht | |
| **Bewerken** | Een verzonden bericht aanpassen | Label op het bericht ‘Bewerkt’. Het Engelse ‘change’ is ‘wijzigen’: ‘Groepsnaam wijzigen’ |
| **Beantwoorden** | Op een bepaald bericht antwoorden | |
| **Reageren** | Een emoji aan een bericht toevoegen | In lopende tekst ‘reageerde met {emoji}’ |
| **Emoji** | Een beeldteken | ‘de emoji’, meervoud ‘emoji's’ |
| **Groep** | Een chat met meerdere leden | De chat zelf is een ‘groepschat’ |

## Acties en interface

| Term | Betekenis | Context |
|------|-----------|---------|
| **Contactverzoek** | Verzoek dat via een gebruikersnaam of een groep wordt verstuurd | Engels ‘contact request’. Het Engelse ‘connect’ tussen mensen is ‘contact maken’. Nooit ‘koppelen’ |
| **Toevoegen** | Iemand aan een groep of aan je contacten toevoegen | |
| **Verwijderen** | Engels ‘remove’ en ‘delete’ | Eén werkwoord voor beide. Het lijdend voorwerp maakt het verschil duidelijk: ‘Uit groep verwijderen’, ‘Bericht verwijderen’. Niet ‘wissen’ |
| **Verlaten** | Zelf uit een groep gaan | ‘Groep verlaten’. Iets anders dan verwijderen, dat over een ander gaat |
| **Blokkeren** | Geen berichten meer van iemand ontvangen | |
| **Deblokkeren** | Een blokkering opheffen | Hetzelfde woord in knoppen en in dialoogtekst |
| **Dempen** | Meldingen van een chat uitzetten | |
| **Dempen opheffen** | Het dempen ongedaan maken | Altijd deze uitdrukking |
| **Koppelen** | Een ander apparaat toegang tot je account geven | Alleen voor apparaten. ‘Gekoppelde apparaten’ |
| **Ontkoppelen** | De toegang van een gekoppeld apparaat intrekken | |
| **Accepteren** | Een contactverzoek aannemen | |
| **Weigeren** | Een contactverzoek afwijzen | De afzender krijgt geen melding |
| **Intrekken** | Een verstuurd contactverzoek terugnemen | ‘Verzoek intrekken’ |
| **Spam melden** | Iemand melden als afzender van ongewenste berichten | Moderatie |
| **Tik** | Engels ‘tap’ | Telefoons en tablets. Ook in een tekst over een ander apparaat |
| **Klik** | Engels ‘click’ | Computer, in de `_desktop`-sleutels |
| **Link** | Engels ‘link’ naar een webadres | ‘Link openen?’. Niet te verwarren met koppelen, dat alleen een werkwoord is |

## Bestanden en gegevens

| Term | Betekenis | Context |
|------|-----------|---------|
| **Byte-eenheden** | B, kB, MB, GB, TB, PB, EB, ZB, YB | Spatie tussen getal en eenheid |
| **Uploaden** | Een bestand naar de server sturen | Zelfstandig naamwoord ‘upload’ |
| **Afbeeldingen** | Een afbeelding uit de galerij kiezen | ‘Camera’ voor een nieuwe foto |

## Status en tijd

| Term | Betekenis | Context |
|------|-----------|---------|
| **Nu** | Op dit moment | Tijdstempel van een heel recent bericht |
| **{count} min** | Minuten geleden | Kort, zonder punt |
| **Vandaag**, **Gisteren** | De huidige en de vorige dag | Datumscheiding in de berichtenlijst |
| **Wordt verzonden** | Het bericht is onderweg naar de server | Berichtstatus. Niet ‘Verzenden’, dat de knop is |
| **Verzenden mislukt** | Het bericht kon niet worden verzonden | Berichtstatus |
| **Verzonden** | Het bericht is op de server aangekomen | Berichtstatus |
| **Afgeleverd** | Het bericht is op het apparaat van de ontvanger aangekomen | Berichtstatus |
| **Gelezen** | De ontvanger heeft het bericht gezien | Berichtstatus |
| **Leesbevestigingen** | De instelling die bepaalt of de leesstatus wordt gedeeld | |
| **Verloopt** | Het verlopen van een contactverzoek | ‘Verloopt over 3 dagen’, ‘Contactverzoek verlopen.’ |

## Instellingen en hulp

| Term | Betekenis | Context |
|------|-----------|---------|
| **Profiel en instellingen** | Het instellingenscherm | |
| **Jij** | Engels ‘You’, het tabblad | Ook in de ledenlijst en bij reacties |
| **Notities voor mezelf** | De notitiechat tussen je eigen apparaten | Engels ‘Notes to self’ |
| **Veiligheidscode** | Code die twee contacten vergelijken om te controleren dat niemand meeluistert | |
| **End-to-end-versleuteling** | Engels ‘end-to-end encryption’ | Bijvoeglijk ‘end-to-end versleuteld’ |
| **Uitnodigingscode** | Code die nodig is om lid te worden van Air | Altijd ‘uitnodigingscode’ |
| **Apparaten** | Engels ‘Devices’, de rij | |
| **Gekoppelde apparaten** | De andere apparaten die op hetzelfde account zijn aangemeld | Het huidige apparaat is ‘Dit apparaat’ |
| **QR-code** | Code om een apparaat te koppelen | |
| **Cijfercode** | Code van alleen cijfers om een apparaat te koppelen | Engels ‘numeric code’ |
| **Server** | De computer waarop het account staat | |
| **Meldingen** | Engels ‘notifications’ | ‘push notifications’ is ‘pushmeldingen’ |
| **Logbestanden** | Diagnostische logs | Engels ‘logs’ |
| **Help** | Hulp en ondersteuning | |
| **Contact opnemen met Air** | Contact met het ondersteuningsteam | |
| **Licenties** | Juridische informatie over opensourceonderdelen | |
| **Versie-informatie** | Technische gegevens over de versie van de app | |
| **OK** | Engels ‘Okay’, de knop | |

## Voor vertalers

- **Air** wordt niet vertaald en niet verbogen, ook niet met een bezits-s
- **Gebruikersnaam** vindt mensen, **weergavenaam** herkent ze in chats
- **Verwijderen** staat voor zowel ‘remove’ als ‘delete’. Het lijdend voorwerp of ‘uit de groep’ maakt duidelijk wat er gebeurt
- **Contact maken** en **contactverzoek** gaan over mensen, **koppelen** gaat over apparaten. Die begrippen lopen niet door elkaar
- **Blokkeren** en **deblokkeren** zijn in alle teksten hetzelfde, ook in dialogen
- Knoppen staan in de infinitief, instructies in de gebiedende wijs
- Nooit een ‘s’ of ‘'s’ achter een placeholder. Schrijf ‘van {displayName}’
- Als de Engelse tekst is veranderd, vertaal je opnieuw vanuit de huidige tekst in plaats van de oude vertaling bij te werken
