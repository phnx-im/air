# Glossário de localização (português do Brasil)

Este glossário define os termos principais da interface do Air para manter as traduções consistentes.

O template `app_en.arb` decide o que cada string diz. Este glossário decide com qual termo ela diz. Quando os dois se contradizem, aponte o conflito em vez de escolher em silêncio.

## Variedade

Este arquivo cobre o português do Brasil. O vocabulário segue o uso brasileiro, então aparecem "usuário", "arquivo", "configurações", "tela", "excluir" e "salvar", nunca as formas europeias correspondentes.

O conteúdo brasileiro fica em `app_pt.arb`, o locale base, porque o CLDR trata o português do Brasil como a variedade não marcada de "pt". É o mesmo arranjo de `app_de.arb`, que traz o alemão da Alemanha sem se chamar `app_de_DE.arb`. Um dispositivo configurado em pt-BR cai neste arquivo.

O português europeu é a variedade marcada e tem arquivos próprios, `app_pt_PT.arb` e `GLOSSARY_pt_PT.md`. As duas variedades não se misturam. Uma escolha que valha para as duas ainda assim é registrada em cada glossário, porque cada um é lido sozinho.

## Registro

O Air trata quem usa o app por "você", em todas as strings, como é padrão nos aplicativos de mensagens brasileiros. O imperativo acompanha o tratamento, na forma "toque", "escolha", "digite".

Onde o imperativo soa seco em um aviso legal ou em um alerta, o tratamento não muda. A frase é reescrita, e não o registro.

## Maiúsculas

Vale a ortografia portuguesa, não a inglesa. O português usa muito menos maiúsculas, então as maiúsculas de um título em inglês não são copiadas. Só levam maiúscula a primeira palavra e os nomes próprios, de modo que "Invite codes" vira "Códigos de convite" e "Safety code" vira "Código de segurança".

Mantêm a maiúscula os nomes próprios, incluindo Air, os nomes de teclas como Enter e os títulos de documentos como Termos de Uso.

## Concordância de gênero

Os nomes que entram nas strings vêm de placeholders, e o app não sabe o gênero de quem eles nomeiam. Uma tradução que faça um adjetivo ou um particípio concordar com um nome vai errar metade das vezes.

A saída é usar verbos em forma finita, não particípios. "{user1} adicionou {user2}" funciona para qualquer par de pessoas, enquanto uma construção com "adicionado" não funcionaria. Onde não houver como evitar a concordância, prefira uma formulação neutra como "esta pessoa".

## Termos básicos do app

| Termo | Definição | Contexto |
|-------|-----------|----------|
| **Air** | O nome do aplicativo | Nunca é traduzido. Permanece em compostos como "conta do Air" |
| **Conta** | A conta de Air de uma pessoa | Aparece como "conta do Air" |
| **Nome de usuário** | Identificador único que se compartilha para se conectar com outras pessoas | Serve apenas para conectar |
| **Nome de exibição** | O nome que aparece nos chats e que as outras pessoas veem | Diferente do nome de usuário. É o que os outros veem quando você escreve |
| **Chat** | Uma conversa entre duas ou mais pessoas | Entre duas pessoas ou em um grupo. Não se traduz como "conversa" |
| **Contato** | Outra pessoa para quem você pode escrever | Aparece como "contato do Air". Grafia brasileira, sem o c |
| **Membro** | Quem participa de um chat em grupo | Usado em contextos de grupo |

## Termos de mensagens

| Termo | Definição | Contexto |
|-------|-----------|----------|
| **Mensagem** | Um texto, uma imagem ou um arquivo enviado em um chat | |
| **Rascunho** | Uma mensagem escrita que ainda não foi enviada | Aparece na lista de chats |
| **Anexo** | Um arquivo ou uma imagem que acompanha uma mensagem | |
| **Escrever** | Redigir uma mensagem nova | |
| **Editar** | Modificar uma mensagem já enviada | |
| **Responder** | Retornar a uma mensagem específica de um chat | |
| **Reagir** | Adicionar um emoji a uma mensagem | Ação do menu de contexto da mensagem |
| **Emoji** | Um único caractere pictográfico | Usado no seletor de reações |
| **Grupo** | Um chat com várias pessoas | |

## Ações e interface

| Termo | Definição | Contexto |
|-------|-----------|----------|
| **Conectar** | Adicionar alguém como contato pelo nome de usuário | Primeiro passo para escrever a alguém novo. Não é o mesmo que vincular |
| **Adicionar** | Incluir alguém em um grupo ou nos contatos | |
| **Remover** | Tirar alguém de um chat em grupo | Usado quando o inglês diz "remove" |
| **Excluir** | Apagar conteúdo em definitivo (mensagens, arquivos e afins) | Usado quando o inglês diz "delete". Não se usa "eliminar", que é a forma europeia |
| **Sair** | Deixar um grupo por decisão própria | Diferente de remover, que se aplica a outra pessoa |
| **Bloquear** | Deixar de receber mensagens de alguém | |
| **Desbloquear** | Desfazer um bloqueio | Uma palavra só, inclusive dentro do texto dos diálogos |
| **Silenciar** | Desativar as notificações de um chat | |
| **Reativar notificações** | Desfazer o silenciamento | Não se usa "dessilenciar", que soa artificial |
| **Vincular** | Dar acesso à conta a outro dispositivo | Vale para dispositivos, não para contatos. Não é o mesmo que conectar |
| **Desvincular** | Retirar o acesso de um dispositivo vinculado | |
| **Denunciar spam** | Sinalizar uma pessoa ou uma mensagem como spam | Recurso de moderação |

## Arquivos e dados

| Termo | Definição | Contexto |
|-------|-----------|----------|
| **Unidades de tamanho** | Medidas de tamanho de arquivo (B, KB, MB, GB e afins) | O português usa as mesmas abreviações do inglês |
| **Tamanho do anexo** | O tamanho do conteúdo enviado | |
| **Carregar** | Mandar um arquivo ou uma imagem para o servidor | Distinto de enviar, que se aplica à mensagem |

## Estado e tempo

| Termo | Definição | Contexto |
|-------|-----------|----------|
| **Agora** | O momento atual | Marca de tempo das mensagens muito recentes |
| **Ontem** | O dia anterior a hoje | Marca de tempo das mensagens de ontem |
| **Enviando** | A mensagem está a caminho do servidor | Indicador de estado da mensagem. O gerúndio é a forma brasileira |
| **Não foi possível enviar** | A mensagem não pôde ser enviada | Indicador de estado da mensagem |
| **Enviada** | A mensagem chegou ao servidor | Indicador de estado da mensagem |
| **Entregue** | A mensagem chegou ao dispositivo de quem recebe | Indicador de estado da mensagem |
| **Lida** | Quem recebeu já leu a mensagem | Indicador de estado da mensagem |
| **Confirmações de leitura** | A configuração que controla se o estado de leitura é compartilhado | Chave das configurações |
| **Editada** | Indica que uma mensagem foi alterada depois de enviada | Aparece junto das mensagens alteradas |

## Configurações e ajuda

| Termo | Definição | Contexto |
|-------|-----------|----------|
| **Configurações** | As opções de configuração do app | A tela se chama "Perfil e configurações" |
| **Perfil** | As informações pessoais de quem usa o app | |
| **Código de segurança** | Código que dois contatos comparam para confirmar que ninguém intercepta o chat | Linha do perfil do contato e tela própria |
| **Código de convite** | Código necessário para entrar no Air | Sempre "código de convite" |
| **Dispositivos vinculados** | Os outros dispositivos com sessão aberta na conta | Seção das configurações |
| **Servidor** | A máquina onde a conta fica hospedada | Escolhido no cadastro e ao vincular |
| **Ajuda** | Seção de suporte e assistência | |
| **Fale com o Air** | Opções para escrever para a equipe | |
| **Licenças** | Informação legal sobre os componentes de código aberto | |
| **Informações da versão** | Detalhes técnicos sobre a versão do app | |

## Notas para quem traduz

- **Air** nunca é traduzido. É o nome do produto e permanece em compostos como "conta do Air"
- **Nome de usuário** e **nome de exibição** não se confundem. O primeiro serve para encontrar pessoas, o segundo para identificá-las nos chats
- **Remover** e **excluir** seguem o verbo do inglês, remover para pessoas e excluir para conteúdo. A tradução não reclassifica o objeto por conta própria
- **Conectar** e **vincular** são coisas diferentes. Conectar adiciona um contato, vincular adiciona um dispositivo
- **Bloquear** e **desbloquear** têm uma palavra cada, inclusive dentro do texto dos diálogos
- O português corre cerca de um quinto mais longo que o inglês, então rótulos curtos merecem uma segunda olhada
