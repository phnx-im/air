# Glosario de localización (español)

Este glosario define los términos clave de la interfaz de Air para mantener traducciones coherentes.

La plantilla `app_en.arb` decide qué dice cada string. Este glosario decide con qué término lo dice. Cuando ambos se contradicen, señálalo en lugar de elegir en silencio.

## Variedad

Se usa un español neutro, válido tanto en España como en América. No se emplean rasgos exclusivos de una región, así que no aparecen ni "ordenador" ni "computadora", ni "móvil" ni "celular", sino "dispositivo". El plural de segunda persona es "ustedes", nunca "vosotros".

Una variante regional tendría sus propios archivos, un ARB propio y un glosario propio, en lugar de mezclarse con el idioma base. El español de España sería `app_es_ES.arb` con `GLOSSARY_es_ES.md`.

Donde ambas variantes usan palabras distintas y ninguna es universal, se elige la forma peninsular por brevedad y se documenta aquí. Es el caso de "añadir" frente a "agregar" y de "ajustes" frente a "configuración". Las dos alternativas se entienden en todas partes.

## Registro

El español tutea en toda la app, en línea con las convenciones de las apps de mensajería. Vale para todos los strings y no varía de uno a otro.

Donde el imperativo resulta brusco en un aviso legal o en una advertencia, se mantiene el tuteo, no se cambia a "usted".

## Mayúsculas

Rige la ortografía española, no la inglesa. El español usa muchas menos mayúsculas, así que las mayúsculas de un título en inglés no se copian. Solo se escriben con mayúscula la primera palabra y los nombres propios, de modo que "Invite codes" es "Códigos de invitación" y "Safety code" es "Código de seguridad".

Conservan su mayúscula los nombres propios, incluidos Air, los nombres de teclas como Enter y los títulos de documentos como Condiciones de uso.

## Signos de interrogación y exclamación

Las preguntas llevan signo de apertura, "¿Eliminar el chat?", aunque el inglés solo cierre. Lo mismo vale para las exclamaciones.

## Tiempos verbales

Los avisos de evento se narran en pretérito indefinido, también cuando el sujeto eres tú, como en "Ana añadió a Luis" o "Reaccionaste con 👍". Los mensajes de sistema cuyo resultado sigue vigente van en pretérito perfecto, como en "Has aceptado la solicitud de contacto" o "Has eliminado este mensaje".

## Términos básicos de la app

| Término | Definición | Contexto |
|---------|-----------|----------|
| **Air** | El nombre de la aplicación | Nunca se traduce. Se mantiene en compuestos como "cuenta de Air" |
| **Cuenta** | La cuenta de Air de una persona | Aparece como "cuenta de Air" |
| **Nombre de usuario** | Identificador único que se comparte para conectar con otras personas | Solo sirve para conectar |
| **Nombre visible** | El nombre que aparece en los chats y que ven las demás personas | Distinto del nombre de usuario. Es lo que ven los demás cuando les escribes |
| **Chat** | Una conversación entre dos o más personas | Entre dos personas o en un grupo. No se traduce como "conversación" |
| **Contacto** | Otra persona a la que puedes escribir | Aparece como "contacto de Air" |
| **Miembro** | Quien participa en un chat de grupo | Se usa en contextos de grupo |
| **Equipo** | Un ordenador de escritorio o portátil | Solo donde hay que distinguirlo del teléfono y de la tablet, como al elegir el método de vinculación. En el resto de los casos se usa "dispositivo" |
| **Tablet** | Un dispositivo táctil de pantalla grande | Se escribe "tablet" y no "tableta", por ser la forma común a todas las regiones |

## Términos de mensajería

| Término | Definición | Contexto |
|---------|-----------|----------|
| **Mensaje** | Un texto, una imagen o un archivo enviado en un chat | |
| **Borrador** | Un mensaje escrito que todavía no se ha enviado | Aparece en la lista de chats |
| **Archivo adjunto** | Un archivo o una imagen que acompaña a un mensaje | "Adjunto" a secas cuando el espacio aprieta |
| **Escribir** | Redactar un mensaje nuevo | |
| **Editar** | Modificar un mensaje ya enviado | |
| **Responder** | Contestar a un mensaje concreto de un chat | |
| **Reaccionar** | Añadir un emoji a un mensaje | Acción del menú contextual del mensaje |
| **Emoji** | Un solo carácter pictográfico | Se usa en el selector de reacciones |
| **Grupo** | Un chat con varias personas | |

## Acciones e interfaz

| Término | Definición | Contexto |
|---------|-----------|----------|
| **Conectar** | Añadir a alguien como contacto mediante su nombre de usuario | Primer paso para escribir a alguien nuevo. No es lo mismo que vincular |
| **Añadir** | Incluir a alguien en un grupo o en los contactos | Se prefiere a "agregar" en todo el archivo |
| **Quitar** | Sacar a alguien de un chat de grupo | Se usa "quitar" cuando el inglés dice "remove" |
| **Eliminar** | Borrar contenido de forma permanente (mensajes, archivos y demás) | Se usa "eliminar" cuando el inglés dice "delete" |
| **Salir** | Abandonar un grupo por decisión propia | A diferencia de "quitar", que se aplica a otra persona |
| **Bloquear** | Dejar de recibir mensajes de alguien | |
| **Desbloquear** | Deshacer un bloqueo | Una sola palabra, también dentro del texto de los diálogos |
| **Silenciar** | Desactivar las notificaciones de un chat | |
| **Dejar de silenciar** | Deshacer un silencio | No se usa "desilenciar" ni "reactivar", que se prestan a confusión |
| **Vincular** | Dar acceso a la cuenta a otro dispositivo | Se aplica a dispositivos, no a contactos. No es lo mismo que conectar |
| **Desvincular** | Retirar el acceso de un dispositivo vinculado | |
| **Reportar spam** | Marcar a una persona o un mensaje como spam | Función de moderación |

## Archivos y datos

| Término | Definición | Contexto |
|---------|-----------|----------|
| **Unidades de tamaño** | Medidas de tamaño de archivo (B, KB, MB, GB y demás) | El español usa las mismas abreviaturas que el inglés |
| **Tamaño del archivo adjunto** | El tamaño del contenido subido | |
| **Subir** | Enviar un archivo o una imagen | |

## Estado y tiempo

| Término | Definición | Contexto |
|---------|-----------|----------|
| **Ahora** | El momento actual | Marca de tiempo de los mensajes muy recientes |
| **Ayer** | El día anterior a hoy | Marca de tiempo de los mensajes de ayer |
| **Enviando** | El mensaje va camino del servidor | Indicador de estado del mensaje |
| **No se pudo enviar** | El mensaje no se pudo enviar | Indicador de estado del mensaje |
| **Enviado** | El mensaje ha llegado al servidor | Indicador de estado del mensaje |
| **Entregado** | El mensaje ha llegado al dispositivo del destinatario | Indicador de estado del mensaje |
| **Leído** | El destinatario ha leído el mensaje | Indicador de estado del mensaje |
| **Confirmaciones de lectura** | El ajuste que controla si se comparte el estado de lectura | Interruptor de los ajustes |
| **Editado** | Indica que un mensaje se modificó después de enviarse | Aparece junto a los mensajes modificados |

## Ajustes y ayuda

| Término | Definición | Contexto |
|---------|-----------|----------|
| **Ajustes** | Las opciones de configuración de la app | La pantalla se llama "Perfil y ajustes" |
| **Perfil** | La información personal de quien usa la app | |
| **Código de seguridad** | Código que dos contactos comparan para confirmar que nadie intercepta su chat | Fila del perfil del contacto y pantalla propia |
| **Código de invitación** | Código que hace falta para entrar en Air | Siempre "código de invitación" |
| **Dispositivos vinculados** | Los demás dispositivos con sesión iniciada en la cuenta | Sección de los ajustes |
| **Servidor** | La máquina donde reside una cuenta | Se elige al registrarse y al vincular |
| **Ayuda** | Sección de soporte y asistencia | |
| **Contactar con Air** | Opciones para escribir al equipo | |
| **Licencias** | Información legal sobre los componentes de código abierto | |
| **Información de versión** | Detalles técnicos sobre la versión de la app | |

## Notas para quien traduce

- **Air** no se traduce nunca. Es el nombre del producto y se mantiene en compuestos como "cuenta de Air"
- **Nombre de usuario** frente a **nombre visible**: el nombre de usuario sirve para encontrar a alguien, el nombre visible para identificarlo en los chats
- **Quitar** frente a **eliminar**: la plantilla inglesa elige el verbo. La traducción sigue ese verbo en lugar de reclasificar el objeto por su cuenta
- **Conectar** frente a **vincular**: conectar añade un contacto, vincular añade un dispositivo. Son dos palabras distintas y no se intercambian
- **Bloquear** y **desbloquear** tienen una sola palabra cada uno, también dentro del texto de los diálogos
- La palabra "delete" que se escribe para confirmar el borrado de la cuenta se queda en inglés, porque la app la compara literalmente
