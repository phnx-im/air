# Glossari de localització (català)

Aquest glossari fixa els termes bàsics de la interfície de l'Air perquè les traduccions siguin coherents.

La plantilla `app_en.arb` defineix què diu cada text. El glossari defineix amb quines paraules ho diu. Si la plantilla i el glossari no coincideixen, indiqueu el conflicte i no trieu cap opció pel vostre compte.

La traducció segueix la [Guia d'estil de Softcatalà](https://www.softcatala.org/guia-estil-de-softcatala/) i la [terminologia de Softcatalà](https://www.softcatala.org/recursos/terminologia/). Quan aquest glossari no resol un dubte, s'apliquen aquestes dues fonts.

## Variant

Es fa servir el català estàndard amb les formes més comunes d'arreu del domini lingüístic. En cas de dubte, per exemple amb certes terminacions verbals, s'opta per les formes generals de la gramàtica de l'IEC. El codi de la traducció és `ca`, el més genèric, i el nom de la llengua al selector d'idioma és «Català».

Una variant té els seus propis fitxers, un ARB propi i un glossari propi, i no es barreja amb la llengua base. El valencià seria la variant «Català (valencià)», amb el seu propi ARB i el seu propi glossari.

## Tractament

L'Air es dirigeix a l'usuari de vós en tots els textos: «Toqueu per a copiar», «Torneu-ho a provar.». La concordança és en singular: «esteu connectat», mai «esteu connectats». Eviteu el pronom «vós» explícit sempre que es pugui. Reformuleu la frase o feu servir els pronoms febles «us» i «vos».

Els botons, els elements de menú, els interruptors i els títols de diàleg que anomenen una acció són ordres de l'usuari a l'aplicació. Van en imperatiu de segona persona del singular: «Desa», «Suprimeix», «Envia amb Retorn», «Canvia el nom del grup». No es fa servir mai l'infinitiu per a donar ordres. Quan l'acció recau sobre l'usuari, el pronom és «em» o «'m», no «et»: «Registra'm a l'Air», no «Uneix-te a l'Air», perquè el botó continua sent una ordre a l'aplicació.

L'excepció és «Vós», que anomena l'usuari mateix a la pestanya «You», a les llistes de membres i a les reaccions.

El to és més formal que en anglès:

- No es tradueixen «please», «sorry», «simply» ni les onomatopeies.
- El possessiu només apareix quan sense ell hi hauria dubte: «el compte», no «el vostre compte».
- Les preguntes són frases completes amb verb conjugat: «Voleu suprimir el xat?», no «Suprimir el xat?».
- «Are you sure» és «Segur que voleu…?», una forma que no marca el gènere.
- «Failed to» i «Couldn't» són «No s'ha pogut…». «Something went wrong» i «An error occurred» són «S'ha produït un error».
- Una acció en curs és «S'està…»: «S'està enviant…», no «Enviant…».
- «May» de probabilitat és «És possible que» + subjuntiu.
- «Should» és «És recomanable que» + subjuntiu.
- La forma activa amb «es» és preferible a la passiva: «S'ha desat el fitxer.».
- La simultaneïtat és «en» + infinitiu, mai «al» + infinitiu: «en obrir l'Air».
- Davant d'infinitiu, la finalitat és «per a» i la causa és «per»: «Toqueu per a copiar».
- La negació amb «cap», «mai», «res» i «ningú» porta «no»: «No s'ha trobat cap paquet.».
- Per a lloc i amb adjectius s'usa «ser»: «El fitxer és massa gran.».

## Gènere

Feu servir la forma o la construcció més genèrica: «Us donem la benvinguda», «tothom», «les amistats». Les formes dobles («segur/a», «benvingut/da») no s'usen. Quan no hi ha cap forma genèrica, el masculí és la forma no marcada.

## Majúscules

S'aplica l'ortografia catalana, no l'anglesa. En títols i botons només porten majúscula la primera paraula i els noms propis, encara que la plantilla anglesa escrigui cada paraula amb majúscula. Els noms dels dies, dels mesos i de les llengües van en minúscula.

## Puntuació

Les cometes són les baixes, «…». Dins d'unes cometes baixes s'usen les altes, "…". Les cometes "..." i “...” de l'anglès passen a «...». L'excepció és `deleteAccountScreen_confirmationInputLabel`, on la paraula «delete» queda sense traduir perquè l'aplicació compara exactament aquesta paraula.

Els textos de la interfície no admeten cursiva ni negreta. Per això, un nom de botó, de pestanya o d'opció dins d'una frase va entre cometes baixes, encara que la plantilla no en porti. Per exemple, toqueu «Enllaça un dispositiu».

No s'usen els signes d'obertura «¿» i «¡». El signe d'admiració de l'anglès se suprimeix, llevat que adverteixi d'una pèrdua de dades.

El text acaba amb el mateix signe que l'original, sigui punt, interrogant, admiració o dos punts. Si l'original no en té, la traducció tampoc. Els punts suspensius van enganxats a la paraula anterior i segueixen la grafia de la plantilla. Si la plantilla té tres punts «...», la traducció també. Si té el caràcter únic «…», la traducció també.

No es posa coma davant de «i» o «o» en una enumeració.

L'apòstrof és el caràcter `'` (U+0027), mai l'accent agut «´».

La ela geminada s'escriu amb el punt volat U+00B7: «cancel·la», «sol·licitud». El punt normal («cancel.la») és incorrecte, i `just check-l10n` el rebutja.

## Nombres i unitats

El nombre i la unitat se separen amb un espai: «5 MB». Les unitats de byte són B, kB, MB, GB, TB, PB, EB, ZB i YB. La «k» de quilo va en minúscula. L'aplicació formata la coma decimal, el punt de milers, la data i l'hora de 24 hores.

La marca de temps per als minuts és «{count} min», amb el símbol «min» separat per un espai.

## Plural

El català fa servir les categories CLDR `one`, `many` i `other`. La categoria `one` només cobreix l'1. La categoria `many` només cobreix els milions exactes i, si falta, l'aplicació fa servir `other`. Per això n'hi ha prou amb `one` i `other`. La branca `other` és obligatòria, i sense ella la comprovació falla. La branca `=0` es redacta sense número quan es llegeix millor: «No hi ha cap dispositiu enllaçat.».

## Noms i apòstrofs

«De», «el» i «la» s'apostrofen davant de vocal i de hac: «d'Anna», «l'Oriol». També s'apostrofen davant dels números que sonen amb vocal, com l'1 i l'11: «l'1 d'octubre». L'aplicació no sap com comença el valor d'un marcador de posició. Per això:

- «De», «el», «la», «d'» i «l'» no van mai just davant d'un marcador de posició. `just check-l10n` ho rebutja.
- El nom pot anar com a subjecte, darrere d'un verb o darrere d'una preposició que no s'apostrofa, com «a», «amb», «per», «en» o «que»: «{displayName} us ha enviat una sol·licitud de contacte.», «Envia una sol·licitud a {displayName}».
- Si cal un article o «de», el porta un nom genèric davant del nom: «el contacte {displayName}», «del membre {user}», «del grup {groupName}», «el dispositiu {device}», «el nom d'usuari {username}».
- El nom genèric és «contacte» quan l'anglès diu «contact» o la persona és un contacte. És «membre» als avisos i diàlegs d'un grup. En els altres casos és «usuari».
- No es fa servir l'article personal («en», «na», «la», «l'») davant dels noms de persona.
- Els pronoms febles «el» i «la» marquen el gènere d'una persona. Repetiu el nom genèric o feu servir «li», que no en marca: «envieu-li una sol·licitud nova».
- Davant d'una data o d'una hora tampoc no hi va l'article, perquè «l'1 d'octubre» i «a la 1:05» no es poden preveure. Reformuleu: «Data d'enllaçament: {date}, {time}».

## Air

Segons la Guia d'estil, els noms de programes porten article: «l'Air», «de l'Air», «a l'Air», «amb l'Air». És masculí i no es flexiona. L'«Air account» és el «compte de l'Air», l'«Air contact» és el «contacte de l'Air» i «on Air» és «a l'Air». Només `appTitle` i el nom de l'aplicació a iOS són «Air» sol.

## Termes bàsics

| Terme | Significat | Context |
|-------|-----------|---------|
| **Air** | El nom de l'aplicació | No es tradueix. Porta article: «l'Air», «de l'Air» |
| **Compte** | El compte d'un usuari a l'Air | «compte de l'Air» |
| **Nom d'usuari** | Identificador únic que compartiu perquè altres persones es posin en contacte amb vós | Només per a trobar persones. Plural «noms d'usuari» |
| **Nom a mostrar** | El nom que veuen els altres als xats | Terme de Softcatalà. És diferent del nom d'usuari |
| **Xat** | Intercanvi de missatges entre dues o més persones | L'anglès «chat». Plural «xats» |
| **Xat de grup** | Xat amb diversos membres | L'anglès «group chat» |
| **Grup** | Xat de grup, quan l'anglès diu «group» | |
| **Contacte** | Un altre usuari a qui podeu enviar missatges | «contacte de l'Air» |
| **Membre** | Participant en un xat de grup | |
| **Persona, persones** | L'anglès «person, people» | «Afegeix persones», «5 persones» |
| **Aplicació** | L'anglès «app» | |
| **Perfil** | El nom a mostrar i la imatge de perfil | |
| **Imatge de perfil** | La imatge que acompanya el nom a mostrar | «imatge del grup» per a un grup |

## Missatges

| Terme | Significat | Context |
|-------|-----------|---------|
| **Missatge** | Text, imatge o fitxer enviat en un xat | |
| **Esborrany** | Missatge escrit però no enviat | A la llista de xats |
| **Adjunt** | Fitxer o imatge enviat amb un missatge | |
| **Edita** | Canviar un missatge que ja s'ha enviat | La marca al missatge és «Editat» |
| **Respon** | Respondre a un missatge concret | |
| **Reacció** | Emoji afegit a un missatge | El verb és «reaccionar» |
| **Emoji** | Caràcter pictogràfic | Plural «emojis» |
| **Missatges nous** | Separador on comencen els missatges no llegits | |

## Accions i interfície

| Terme | Significat | Context |
|-------|-----------|---------|
| **Sol·licitud de contacte** | Petició enviada mitjançant un nom d'usuari o un grup | L'anglès «connect» per a persones és «posar-se en contacte». Mai «connectar», que és per al servidor |
| **Afegeix** | Afegir una persona a un grup o als contactes | |
| **Elimina** | L'anglès «remove» | Treure una persona d'un grup, un nom d'usuari, una reacció o els codis utilitzats |
| **Suprimeix** | L'anglès «delete» | Esborrar contingut de manera definitiva: missatge, xat, contacte, compte. No tradueix «remove» |
| **Abandona** | Sortir d'un grup per decisió pròpia | «Abandona el grup». És diferent d'eliminar, que s'aplica a una altra persona |
| **Bloca** | Deixar de rebre missatges d'algú | Verb «blocar», participi «blocat». Mai «bloquejar» |
| **Desbloca** | Posar fi al bloqueig | Verb «desblocar» |
| **Silencia** | Desactivar les notificacions d'un xat | |
| **Activa les notificacions** | Tornar a activar les notificacions d'un xat | L'anglès «unmute». Silenciar un xat afecta les notificacions, no només el so |
| **Enllaça** | Donar accés al compte a un altre dispositiu | Només per a dispositius. Participi «enllaçat», nom «enllaçament» |
| **Desenllaça** | Retirar l'accés a un dispositiu enllaçat | Només per a dispositius |
| **Accepta** | Acceptar una sol·licitud de contacte | |
| **Rebutja** | Rebutjar una sol·licitud de contacte | No s'avisa el remitent |
| **Retira** | Fer enrere una sol·licitud de contacte que heu enviat | |
| **Denuncia brossa** | Indicar que una persona envia brossa | L'anglès «spam» és «brossa». El nom és «denúncia de brossa». «Informa» queda per als errors i les incidències |
| **Toqueu** | L'anglès «tap» | Telèfons i tauletes |
| **Feu clic** | L'anglès «click» | Ordinadors, a les claus `_desktop` |
| **Introduïu** | L'anglès «enter» per a un codi | «Escriviu» per a «type» |

## Fitxers i dades

| Terme | Significat | Context |
|-------|-----------|---------|
| **Unitats de byte** | B, kB, MB, GB, TB, PB, EB, ZB, YB | Nombre i unitat separats per un espai |
| **Puja** | Enviar un fitxer al servidor | Verb «pujar», nom «pujada» |
| **Imatges** | Triar una imatge de la galeria | «Càmera» per a fer una foto nova |
| **Porta-retalls** | L'anglès «clipboard» | «S'ha copiat al porta-retalls» |

## Estats i temps

| Terme | Significat | Context |
|-------|-----------|---------|
| **Ara** | En aquest moment | Marca de temps d'un missatge molt recent |
| **{count} min** | Fa uns minuts | Breu, sense punt |
| **Avui**, **Ahir** | El dia d'avui i el d'ahir | Separadors de data a la llista de missatges |
| **S'està enviant** | El missatge va cap al servidor | Estat del missatge |
| **No s'ha pogut enviar** | El missatge no s'ha enviat | Estat del missatge |
| **Enviat** | El missatge ha arribat al servidor | Estat del missatge |
| **Lliurat** | El missatge ha arribat al dispositiu del destinatari | Estat del missatge |
| **Llegit** | El destinatari ha llegit el missatge | Estat del missatge |
| **Confirmacions de lectura** | El paràmetre que decideix si compartiu l'estat de lectura | |
| **Caduca** | Venciment d'una sol·licitud de contacte | «Caduca d'aquí a 3 dies», «La sol·licitud de contacte ha caducat.» |

## Paràmetres i ajuda

| Terme | Significat | Context |
|-------|-----------|---------|
| **Perfil i paràmetres** | La pantalla dels paràmetres | L'anglès «settings» és «paràmetres» |
| **Vós** | La pestanya anglesa «You» | També a les llistes de membres i de reaccions |
| **Codi de seguretat** | Codi que comparen dos contactes per a comprovar que ningú no intercepta el xat | |
| **Xifratge d'extrem a extrem** | L'anglès «end-to-end encryption» | El participi és «xifrat d'extrem a extrem» |
| **Codi d'invitació** | Codi que cal per a registrar-se a l'Air | Sempre «codi d'invitació» |
| **Dispositius** | La fila anglesa «Devices» | |
| **Dispositius enllaçats** | Els altres dispositius enllaçats al mateix compte | El dispositiu actual és «Aquest dispositiu» |
| **Codi QR** | Codi per a enllaçar un dispositiu | |
| **Codi numèric** | Codi de xifres per a enllaçar un dispositiu | |
| **Servidor** | L'ordinador que allotja el compte | |
| **Notificacions** | L'anglès «notifications» | L'anglès «push notifications» és «notificacions push» |
| **Registres** | Fitxers de diagnòstic | L'anglès «logs» |
| **Ajuda** | Ajuda i assistència | |
| **Contacteu amb l'Air** | Contactar amb l'equip de l'Air | |
| **Llicències** | Informació legal sobre els components de codi obert | |
| **Informació de la versió** | Dades tècniques de la versió de l'aplicació | |
| **D'acord** | El botó anglès «Okay» | |
| **Cancel·la** | El botó anglès «Cancel» | |

## Notes per a qui tradueix

- L'**Air** no es tradueix i porta article: «l'Air», «de l'Air», «a l'Air»
- El **nom d'usuari** serveix per a trobar persones, el **nom a mostrar** per a identificar-les als xats
- **Elimina** i **suprimeix** segueixen el verb anglès. «Remove» és sempre «elimina» i «delete» és sempre «suprimeix»
- La **sol·licitud de contacte** i **posar-se en contacte** són per a persones. **Enllaçar** és per a dispositius i **connectar** per al servidor. Els conceptes no es barregen
- Els botons van en imperatiu de segona persona del singular, els missatges es dirigeixen a l'usuari de vós
- «De», «el», «la», «d'» i «l'» no van mai davant d'un marcador de posició. Feu servir un nom genèric o reformuleu
- Si el text anglès ha canviat, torneu a traduir-lo a partir del text actual i no corregiu la traducció antiga
