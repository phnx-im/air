# Glossário de localização (português europeu)

Este glossário define os termos principais da interface do Air para manter as traduções consistentes.

O template `app_en.arb` decide o que cada string diz. Este glossário decide com que termo o diz. Quando os dois se contradizem, assinala o conflito em vez de escolheres em silêncio.

## Variedade

Este ficheiro cobre o português europeu. O vocabulário segue o uso de Portugal, pelo que aparecem "utilizador", "ficheiro", "definições", "ecrã", "eliminar" e "guardar", nunca as formas brasileiras correspondentes.

O português do Brasil fica no locale base, em `app_pt.arb` e `GLOSSARY_pt.md`, porque o CLDR trata o português do Brasil como a variedade não marcada de "pt". As duas variedades não se misturam. Uma escolha que sirva as duas continua a ser registada em cada glossário, porque cada um é lido sozinho.

Como o europeu é a variedade marcada, só um dispositivo configurado em pt-PT cai neste ficheiro. Angola, Moçambique e os restantes locales lusófonos caem no base, ou seja, no brasileiro. É o comportamento herdado do CLDR, e mudá-lo exigiria uma resolução de locale própria.

A grafia segue o Acordo Ortográfico de 1990, que mantém o c de "contacto" em Portugal e o retira de "ação" e "ativar" nas duas variedades.

## Registo

O Air trata quem usa a aplicação por tu, em todas as strings, como é hábito nas aplicações de mensagens portuguesas. O imperativo acompanha o tratamento, na forma "toca", "escolhe", "escreve", e os possessivos são "o teu" e "a tua".

Onde o imperativo soa seco num aviso legal ou num alerta, o tratamento não muda. Reescreve-se a frase, não o registo.

## Maiúsculas

Vale a ortografia portuguesa, não a inglesa. O português usa muito menos maiúsculas, pelo que as maiúsculas de um título em inglês não se copiam. Só levam maiúscula a primeira palavra e os nomes próprios, de modo que "Invite codes" fica "Códigos de convite" e "Safety code" fica "Código de segurança".

Mantêm a maiúscula os nomes próprios, incluindo Air, os nomes de teclas como Enter e os títulos de documentos como Termos de Utilização.

## Gerúndio

O português europeu não usa o gerúndio para a ação em curso. A forma corrente é "a" mais infinitivo, pelo que "Sending" fica "A enviar" e "Connecting" fica "A ligar". O gerúndio brasileiro não aparece neste ficheiro.

## Concordância de género

Os nomes que entram nas strings vêm de placeholders, e a aplicação não sabe o género de quem eles nomeiam. Uma tradução que faça um adjetivo ou um particípio concordar com um nome erra metade das vezes.

A saída é usar verbos em forma finita, não particípios. "{user1} adicionou {user2}" serve qualquer par de pessoas, ao passo que uma construção com "adicionado" não serviria. Onde não houver como evitar a concordância, prefere uma formulação neutra como "esta pessoa".

## Termos básicos da aplicação

| Termo | Definição | Contexto |
|-------|-----------|----------|
| **Air** | O nome da aplicação | Nunca se traduz. Mantém-se em compostos como "conta do Air" |
| **Conta** | A conta de Air de uma pessoa | Aparece como "conta do Air" |
| **Nome de utilizador** | Identificador único que se partilha para conectar com outras pessoas | Serve apenas para conectar |
| **Nome a apresentar** | O nome que aparece nos chats e que as outras pessoas veem | Diferente do nome de utilizador. É o que os outros veem quando lhes escreves |
| **Chat** | Uma conversa entre duas ou mais pessoas | Entre duas pessoas ou num grupo. Não se traduz por "conversa" |
| **Contacto** | Outra pessoa a quem podes escrever | Aparece como "contacto do Air". Grafia europeia, com o c |
| **Membro** | Quem participa num chat de grupo | Usa-se em contextos de grupo |

## Termos de mensagens

| Termo | Definição | Contexto |
|-------|-----------|----------|
| **Mensagem** | Um texto, uma imagem ou um ficheiro enviado num chat | |
| **Rascunho** | Uma mensagem escrita que ainda não foi enviada | Aparece na lista de chats |
| **Anexo** | Um ficheiro ou uma imagem que acompanha uma mensagem | |
| **Escrever** | Redigir uma mensagem nova | |
| **Editar** | Alterar uma mensagem já enviada | |
| **Responder** | Retomar uma mensagem específica de um chat | |
| **Reagir** | Juntar um emoji a uma mensagem | Ação do menu de contexto da mensagem |
| **Emoji** | Um único carácter pictográfico | Usa-se no seletor de reações |
| **Grupo** | Um chat com várias pessoas | |

## Ações e interface

| Termo | Definição | Contexto |
|-------|-----------|----------|
| **Conectar** | Juntar alguém aos contactos através do nome de utilizador | Primeiro passo para escrever a alguém novo. Não é o mesmo que associar. Não se usa "ligar", que em Portugal se lê como telefonar |
| **Adicionar** | Incluir alguém num grupo ou nos contactos | |
| **Remover** | Tirar alguém de um chat de grupo | Usa-se quando o inglês diz "remove" |
| **Eliminar** | Apagar conteúdo em definitivo (mensagens, ficheiros e afins) | Usa-se quando o inglês diz "delete". Não se usa "excluir", que é a forma brasileira |
| **Sair** | Deixar um grupo por decisão própria | Diferente de remover, que se aplica a outra pessoa |
| **Bloquear** | Deixar de receber mensagens de alguém | |
| **Desbloquear** | Desfazer um bloqueio | Uma só palavra, inclusive dentro do texto das caixas de diálogo |
| **Silenciar** | Desativar as notificações de um chat | |
| **Reativar notificações** | Desfazer o silenciamento | Não se usa "dessilenciar", que soa artificial |
| **Associar** | Dar acesso à conta a outro dispositivo | Vale para dispositivos, não para contactos. Não é o mesmo que conectar |
| **Desassociar** | Retirar o acesso a um dispositivo associado | |
| **Denunciar spam** | Assinalar uma pessoa ou uma mensagem como spam | Funcionalidade de moderação |

## Ficheiros e dados

| Termo | Definição | Contexto |
|-------|-----------|----------|
| **Unidades de tamanho** | Medidas de tamanho de ficheiro (B, KB, MB, GB e afins) | O português usa as mesmas abreviaturas do inglês |
| **Tamanho do anexo** | O tamanho do conteúdo carregado | |
| **Carregar** | Mandar um ficheiro ou uma imagem para o servidor | Distinto de enviar, que se aplica à mensagem |

## Estado e tempo

| Termo | Definição | Contexto |
|-------|-----------|----------|
| **Agora** | O momento atual | Marca de tempo das mensagens muito recentes |
| **Ontem** | O dia anterior a hoje | Marca de tempo das mensagens de ontem |
| **A enviar** | A mensagem vai a caminho do servidor | Indicador de estado da mensagem. Não se usa o gerúndio |
| **Não foi possível enviar** | A mensagem não pôde ser enviada | Indicador de estado da mensagem |
| **Enviada** | A mensagem chegou ao servidor | Indicador de estado da mensagem |
| **Entregue** | A mensagem chegou ao dispositivo de quem recebe | Indicador de estado da mensagem |
| **Lida** | Quem recebeu já leu a mensagem | Indicador de estado da mensagem |
| **Confirmações de leitura** | A definição que controla se o estado de leitura é partilhado | Interruptor das definições |
| **Editada** | Indica que uma mensagem foi alterada depois de enviada | Aparece junto das mensagens alteradas |

## Definições e ajuda

| Termo | Definição | Contexto |
|-------|-----------|----------|
| **Definições** | As opções de configuração da aplicação | O ecrã chama-se "Perfil e definições" |
| **Perfil** | As informações pessoais de quem usa a aplicação | |
| **Código de segurança** | Código que dois contactos comparam para confirmar que ninguém interceta o chat | Linha do perfil do contacto e ecrã próprio |
| **Código de convite** | Código necessário para entrar no Air | Sempre "código de convite" |
| **Dispositivos associados** | Os outros dispositivos com sessão iniciada na conta | Secção das definições |
| **Servidor** | A máquina onde a conta fica alojada | Escolhido no registo e ao associar |
| **Ajuda** | Secção de apoio e assistência | |
| **Contactar o Air** | Opções para escrever à equipa | |
| **Licenças** | Informação legal sobre os componentes de código aberto | |
| **Informações da versão** | Detalhes técnicos sobre a versão da aplicação | |

## Notas para quem traduz

- **Air** nunca se traduz. É o nome do produto e mantém-se em compostos como "conta do Air"
- **Nome de utilizador** e **nome a apresentar** não se confundem. O primeiro serve para encontrar pessoas, o segundo para as identificar nos chats
- **Remover** e **eliminar** seguem o verbo do inglês, remover para pessoas e eliminar para conteúdo. A tradução não reclassifica o objeto por conta própria
- **Conectar** e **associar** são coisas diferentes. Conectar junta um contacto, associar junta um dispositivo. "A ligar" fica reservado para a ligação ao servidor
- **Bloquear** e **desbloquear** têm uma palavra cada, inclusive dentro do texto das caixas de diálogo
- O português corre cerca de um quinto mais longo do que o inglês, pelo que os rótulos curtos merecem uma segunda leitura
