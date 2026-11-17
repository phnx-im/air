# Glossario di localizzazione (italiano)

Questo glossario stabilisce i termini di base dell'interfaccia di Air, in modo che le traduzioni restino coerenti.

Il modello `app_en.arb` stabilisce che cosa dice ogni testo. Il glossario stabilisce con quali termini lo dice. Se il modello e il glossario non concordano, segnala il conflitto invece di scegliere da solo.

## Variante

Si usa l'italiano standard. Il codice della traduzione è `it` e il nome della lingua nel selettore è “Italiano”.

Una variante ha i propri file, un ARB proprio e un glossario proprio, e non si mescola con la lingua di base. L'italiano della Svizzera sarebbe `app_it_CH.arb` con `GLOSSARY_it_CH.md`.

## Registro

Air si rivolge all'utente con il “tu” in tutti i testi, come Android e Apple. Il registro non cambia da un testo all'altro.

I pulsanti e le voci di menu usano l'imperativo alla seconda persona singolare: “Elimina”, “Invia”, “Annulla”, “Riprova”. Anche le istruzioni e i messaggi di errore usano l'imperativo: “Riprova.”, “Inserisci il codice”.

I titoli di conferma formulati come domanda usano “Vuoi” più l'infinito: “Vuoi eliminare il messaggio?”, “Vuoi bloccare {displayName}?”.

Formule ricorrenti:

- “Are you sure you want to…” diventa “Vuoi davvero…?”, che non marca il genere.
- “Failed to…” e “Couldn't…” diventano “Impossibile…”: “Impossibile inviare il messaggio. Riprova.”
- Un'azione in corso è “… in corso”: “Invio in corso…”, “Collegamento in corso…”.
- Se il testo inglese non contiene “please”, la traduzione non aggiunge “per favore”.

## Genere

L'app non conosce il genere dell'utente né quello delle persone nominate nei segnaposto. Una traduzione che fa concordare un participio o un aggettivo con una persona sbaglia una volta su due. Per questo:

- Usa forme verbali finite e tempi composti con “avere”, che non concordano: “{user1} ha aggiunto {user2}”, “Hai eliminato questo messaggio.”
- Evita participi e aggettivi riferiti a persone: “Vuoi davvero chiudere?” e non “Sei sicuro di voler chiudere?”.
- Evita anche i nomi maschili riferiti all'utente: “Non fai più parte di questa chat.” e non “Non sei più membro di questa chat.”
- Quando l'inglese usa un passivo o un verbo con “essere” riferito a una persona, passa all'attivo con il soggetto “qualcuno”: “{user1} was added to the chat” diventa “Qualcuno ha aggiunto {user1} alla chat”, “{user1} was removed from the chat” diventa “Qualcuno ha rimosso {user1} dalla chat”.
- Le forme doppie come “sicuro/a” o “stato/a” non si usano. Se non esiste una forma neutra, si usa il maschile come forma non marcata.
- I participi riferiti a cose concordano normalmente: “Messaggio eliminato”, “Allegato salvato”.

## Maiuscole

Si applica l'ortografia italiana, non quella inglese. Nei titoli e nei pulsanti solo la prima parola e i nomi propri hanno la maiuscola, anche se il modello inglese scrive ogni parola con la maiuscola: “Codici di invito”, “Codice di sicurezza”. Giorni, mesi e nomi delle lingue sono minuscoli. I titoli di documenti come “Termini di utilizzo” hanno la maiuscola solo sulla prima parola.

## Punteggiatura

Le virgolette sono quelle alte doppie, “…”. Al loro interno si usano quelle semplici, ‘…’. Le virgolette inglesi "..." e “...” diventano “...” nella traduzione. Fa eccezione `deleteAccountScreen_confirmationInputLabel`, dove la parola 'delete' resta in inglese tra apici semplici, perché l'app confronta esattamente quella parola.

Il nome di un pulsante, di una scheda o di un'opzione dentro una frase va tra virgolette, anche se il modello non le usa: tocca “Collega un dispositivo”.

Il testo finisce con lo stesso segno dell'originale, che sia un punto, un punto interrogativo, un punto esclamativo o i due punti. Se l'originale non ha un segno finale, non lo ha nemmeno la traduzione. I puntini di sospensione seguono la grafia del modello. Se il modello ha tre punti “...”, la traduzione ha tre punti. Se ha il carattere unico “…”, la traduzione ha quel carattere.

Il separatore del percorso di menu resta quello del modello, “>” oppure “→”.

## Numeri e unità

Numero e unità sono separati da uno spazio: “5 MB”. Le unità di byte sono B, kB, MB, GB, TB, PB, EB, ZB e YB, con la “k” minuscola come in Android. Il separatore decimale è la virgola e lo formatta l'app.

Il timestamp per i minuti è “{count} min”, senza punto.

## Plurale

L'italiano usa le categorie CLDR `one`, `many` e `other`. La categoria `one` copre solo l'1. La categoria `many` copre solo i milioni esatti e, se manca, l'app usa `other`. Bastano quindi `one` e `other`. Il ramo `other` è obbligatorio e senza di esso il controllo fallisce. Il ramo `=0` si formula senza numero quando si legge meglio: “Nessun dispositivo collegato.”

Se il testo è uguale in tutte le forme, basta un solo ramo `other`.

## Nomi, articoli e preposizioni

L'app non sa come inizia il valore di un segnaposto né il genere di ciò che nomina. Per questo:

- Davanti a un segnaposto non vanno articoli né preposizioni articolate. Non si scrive “il {groupName}”, “dal {groupName}” o “l'{device}”.
- Se serve un articolo, lo porta un nome generico davanti al segnaposto: “il gruppo {groupName}”, “dal gruppo {groupName}”, “la chat {chatName}”, “il dispositivo {device}”, “il nome utente {username}”.
- Davanti ai nomi di persona non si usa l'articolo. Le preposizioni semplici vanno bene: “a {displayName}”, “di {displayName}”, “da {displayName}”, “con {displayName}”. Davanti a un segnaposto si scrive sempre “a”, mai la forma eufonica “ad”.
- “{displayName}'s contact request” diventa “la richiesta di contatto di {displayName}”.
- I pronomi “lo”, “la”, “gli” e “le” rivelano il genere della persona. L'inglese “they” o “them” riferito a una persona diventa “questa persona”, oppure si ripete il nome: “finché non sblocchi questa persona”.

## Air

“Air” non si traduce e non prende l'articolo: “su Air”, “di Air”, “con Air”. Davanti ad “Air” si scrive “ad”: “Unisciti ad Air”. “Air account” è “account Air”, “Air contact” è “contatto Air”, “on Air” è “su Air” e “Contact Air” è “Contatta Air”.

## Termini di base

| Termine | Significato | Contesto |
|---------|-------------|----------|
| **Air** | Il nome dell'app | Non si traduce e non prende l'articolo |
| **Account** | L'account di un utente su Air | “account Air”. Maschile invariabile: “l'account”, “gli account” |
| **Nome utente** | Identificativo unico che si condivide per farsi contattare | Solo per mettersi in contatto. Plurale “nomi utente” |
| **Nome visualizzato** | Il nome che gli altri vedono nelle chat | Inglese “display name”. Diverso dal nome utente. Non “nome profilo” |
| **Chat** | Conversazione tra due o più persone | Femminile invariabile: “la chat”, “le chat”. Non “conversazione” |
| **Contatto** | Un altro utente a cui puoi scrivere | “contatto Air” |
| **Membro** | Partecipante a una chat di gruppo | Nei gruppi. Maschile anche per le donne: “un membro”. Non si usa rivolgendosi all'utente |
| **Persona, persone** | Inglese “person, people” | “Aggiungi persone”, “5 persone”. Anche per “they” e “them” riferiti a una persona |
| **Utente** | Chiunque usi Air | Nome generico davanti a un segnaposto |
| **App** | Inglese “app” | Femminile: “l'app”. Non “applicazione” |

## Messaggi

| Termine | Significato | Contesto |
|---------|-------------|----------|
| **Messaggio** | Testo, immagine o file inviato in una chat | |
| **Elimina per me**, **Elimina per tutti** | Eliminare un messaggio solo dai propri dispositivi o per tutti nella chat | |
| **Bozza** | Messaggio scritto ma non ancora inviato | Nell'elenco delle chat |
| **Allegato** | File o immagine inviati con un messaggio | |
| **Modifica** | Cambiare un messaggio già inviato | Etichetta sul messaggio “Modificato”. Anche l'inglese “change” è “modifica”: “Modifica il nome del gruppo” |
| **Rispondi** | Rispondere a un messaggio specifico | |
| **Reagisci** | Aggiungere un'emoji a un messaggio | Nel testo “ha reagito con {emoji}” |
| **Emoji** | Carattere pittografico | Femminile invariabile: “un'emoji”, “le emoji” |
| **Gruppo** | Chat con più membri | La chat stessa è la “chat di gruppo” |

## Azioni e interfaccia

| Termine | Significato | Contesto |
|---------|-------------|----------|
| **Richiesta di contatto** | Richiesta inviata tramite un nome utente o un gruppo | Inglese “contact request”. L'inglese “connect” tra persone è “mettersi in contatto”. Mai “collegare” |
| **Aggiungi** | Inserire qualcuno in un gruppo o tra i contatti | |
| **Rimuovi** | Inglese “remove” | Una persona da un gruppo, ma anche nomi utente, reazioni e codici usati |
| **Elimina** | Inglese “delete” | Contenuti, in modo definitivo: messaggio, chat, contatto, account. Non “cancella” |
| **Abbandona** | Uscire da un gruppo di propria iniziativa | “Abbandona il gruppo”. Diverso da “rimuovi”, che riguarda un'altra persona |
| **Blocca** | Smettere di ricevere messaggi da qualcuno | |
| **Sblocca** | Annullare un blocco | Stessa parola nei pulsanti e nel testo dei dialoghi |
| **Silenzia** | Sospendere le notifiche di una chat | |
| **Riattiva notifiche** | Annullare il silenziamento | Sempre questa espressione |
| **Collega** | Dare a un altro dispositivo accesso al proprio account | Solo per i dispositivi. “Dispositivi collegati”, “collegamento” |
| **Scollega** | Revocare l'accesso di un dispositivo collegato | |
| **Accetta** | Accettare una richiesta di contatto | |
| **Rifiuta** | Rifiutare una richiesta di contatto | Il mittente non viene avvisato |
| **Ritira** | Annullare una richiesta di contatto inviata | “Ritira la richiesta” |
| **Segnala spam** | Segnalare qualcuno come mittente di messaggi indesiderati | Moderazione |
| **Tocca** | Inglese “tap” | Telefoni e tablet. Anche in un testo che descrive un altro dispositivo |
| **Fai clic** | Inglese “click” | Computer, nelle chiavi `_desktop` |
| **Link** | Inglese “link” verso un indirizzo web | “Vuoi aprire il link?”. Non si confonde con “collega”, che è un verbo |

## File e dati

| Termine | Significato | Contesto |
|---------|-------------|----------|
| **Unità di byte** | B, kB, MB, GB, TB, PB, EB, ZB, YB | Spazio tra numero e unità |
| **Caricare** | Inviare un file al server | Inglese “upload”. Il sostantivo è “caricamento” |
| **Immagini** | Scegliere un'immagine dalla galleria | “Fotocamera” per scattare una nuova foto |

## Stati e tempo

| Termine | Significato | Contesto |
|---------|-------------|----------|
| **Ora** | In questo momento | Timestamp di un messaggio molto recente |
| **{count} min** | Minuti fa | Breve, senza punto |
| **Oggi**, **Ieri** | Il giorno attuale e quello precedente | Separatori di data nell'elenco dei messaggi |
| **Invio in corso** | Il messaggio sta andando al server | Stato del messaggio |
| **Invio non riuscito** | Il messaggio non è stato inviato | Stato del messaggio |
| **Inviato** | Il messaggio è arrivato al server | Stato del messaggio |
| **Consegnato** | Il messaggio è arrivato sul dispositivo del destinatario | Stato del messaggio |
| **Letto** | Il destinatario ha visto il messaggio | Stato del messaggio |
| **Conferme di lettura** | L'impostazione che decide se condividere lo stato di lettura | |
| **Scade** | La scadenza di una richiesta di contatto | “Scade tra 3 giorni”, “Richiesta di contatto scaduta.” |

## Impostazioni e aiuto

| Termine | Significato | Contesto |
|---------|-------------|----------|
| **Profilo e impostazioni** | La schermata delle impostazioni | |
| **Tu** | Inglese “You”, la scheda | Anche nell'elenco dei membri e delle reazioni |
| **Note personali** | La chat di note tra i propri dispositivi | Inglese “Notes to self” |
| **Codice di sicurezza** | Codice che due contatti confrontano per verificare che nessuno intercetti la chat | |
| **Crittografia end-to-end** | Inglese “end-to-end encryption” | Aggettivo “crittografato end-to-end”, che concorda con i messaggi |
| **Codice di invito** | Codice necessario per unirsi ad Air | Sempre “codice di invito” |
| **Dispositivi** | Inglese “Devices”, la voce | |
| **Dispositivi collegati** | Gli altri dispositivi collegati allo stesso account | Il dispositivo in uso è “Questo dispositivo” |
| **Codice QR** | Codice per collegare un dispositivo | |
| **Codice numerico** | Codice di sole cifre per collegare un dispositivo | |
| **Server** | Il computer che ospita l'account | Invariabile |
| **Notifiche** | Inglese “notifications” | “push notifications” è “notifiche push” |
| **Log** | Registri diagnostici | Invariabile: “i log” |
| **Aiuto** | Aiuto e supporto | |
| **Contatta Air** | Contattare il team di supporto | |
| **Licenze** | Informazioni legali sui componenti open source | |
| **Informazioni sulla versione** | Dettagli tecnici sulla versione dell'app | |
| **OK** | Inglese “Okay”, il pulsante | |

## Per chi traduce

- **Air** non si traduce e non prende l'articolo: “su Air”, “ad Air”
- **Nome utente** serve a trovare le persone, **nome visualizzato** a riconoscerle nelle chat
- **Rimuovi** ed **elimina** seguono il verbo inglese. “remove” è sempre “rimuovi”, “delete” è sempre “elimina”
- **Mettersi in contatto** e **richiesta di contatto** riguardano le persone, **collega** riguarda i dispositivi. I concetti non si mescolano
- **Blocca** e **sblocca** restano uguali in tutti i testi, anche nei dialoghi
- Niente participi o aggettivi riferiti a persone e niente forme doppie. Un passivo riferito a una persona diventa “Qualcuno ha…”. Il maschile solo se non esiste una forma neutra
- Nessun articolo davanti a un segnaposto. Lo porta un nome generico: “il gruppo {groupName}”
- Se il testo inglese è cambiato, traduci di nuovo dal testo attuale invece di correggere la traduzione vecchia
