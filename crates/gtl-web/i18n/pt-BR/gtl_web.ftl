# Viewer copy for pt-BR. Every message and argument must match the en-US
# catalog. Keep Git and developer jargon (commit, push, branch, diff, snapshot,
# upstream, HEAD, live) in English inside Portuguese sentences. Add a [0]
# variant where a count can be zero: CLDR treats zero as singular in Portuguese.

## Shared actions

action-retry = Tentar de novo
action-try-again = Tentar novamente

## Dates

date-just-now = agora mesmo
date-minutes-ago =
    { $count ->
        [one] há { $count } minuto
       *[other] há { $count } minutos
    }
date-hours-ago =
    { $count ->
        [one] há { $count } hora
       *[other] há { $count } horas
    }
date-days-ago =
    { $count ->
        [one] há { $count } dia
       *[other] há { $count } dias
    }

## Document titles

document-title-projects = Projetos - git-tools
document-title-settings = Configurações - git-tools
document-title-viewer = Visualizador - git-tools

## Application navigation and window chrome

navigation-label = Navegação do visualizador
navigation-projects = Projetos
navigation-open-diffs = Diffs abertos
navigation-no-open-diffs = Nenhum diff aberto
window-drag-region = Arraste para mover a janela
window-controls = Controles da janela
window-minimize = Minimizar janela
window-maximize = Maximizar janela
window-restore = Restaurar janela
window-close = Fechar janela

## Server connection

connection-connecting = Conectando ao servidor do visualizador…
connection-retrying = { $message } Tentando novamente automaticamente.
connection-try-now = Tentar agora
feedback-snapshots-skipped-named =
    { $count ->
        [one] { $count } diff sem commits nem arquivos alterados foi ignorado: { $labels }.
       *[other] { $count } diffs sem commits nem arquivos alterados foram ignorados: { $labels }.
    }

## User settings page

settings-title = Configurações do usuário
settings-loading = Carregando configurações
settings-unavailable = As configurações estão indisponíveis
settings-configuration-file = Arquivo de configuração
settings-edit-stale = As configurações mudaram desde que esta página foi carregada. Recarregue-as antes de salvar de novo.
settings-edit-field-rejected = Corrija a configuração destacada e tente de novo.
settings-edit-rejected = Uma ou mais configurações foram rejeitadas. Recarregue os valores salvos e tente de novo.
settings-edit-invalid-file = O arquivo de configurações ficou inválido. Recarregue-o para corrigi-lo ou redefini-lo.

## Viewer settings form

settings-language = Idioma
settings-date-format = Formato de data
settings-date-format-hint = As datas usam seu fuso horário. Diffs HTML salvos mantêm datas ISO.
settings-date-format-iso = ISO ({ $sample })
settings-date-format-day-first = Dia primeiro ({ $sample })
settings-date-format-month-first = Mês primeiro ({ $sample })
settings-date-format-relative = Relativo ({ $sample })
settings-ui-scale = Tamanho da interface
settings-reduce-motion = Movimento reduzido
settings-reduce-motion-hint = Sempre reduzir animações. Desmarque para seguir a preferência do sistema.
settings-theme = Tema
settings-theme-default = Padrão embutido (Dark)
settings-layout = Layout
settings-layout-unified = Unificado
settings-layout-split = Lado a lado
settings-wrap-lines = Quebrar linhas
settings-wrap-lines-hint = Ajusta as linhas de código à largura disponível.
settings-copy-with-line-context = Copiar com contexto de linha
settings-copy-with-line-context-hint = Inclui o caminho do arquivo e os números das linhas ao copiar código de um diff.
settings-density = Exibição
settings-density-compact = Só as alterações
settings-density-full = Arquivo completo
settings-focus-window = Focar a janela ao abrir um diff
settings-focus-window-hint = Traz a janela do desktop para a frente quando um comando abre um diff.
settings-push-confirmation = Sem confirmação de push na CLI
settings-viewer-push-confirmation = Sem confirmação de push no viewer
settings-viewer-push-confirmation-hint = Quando ativado, o push no viewer é imediato. Desativado por padrão para evitar pushes acidentais.
settings-push-confirmation-hint = Faça push imediatamente pela CLI. Desativado por padrão para evitar pushes acidentais.
settings-field-correction = Valor não suportado. Escolha outra opção.
settings-reload = Recarregar configurações

## Projects

projects-import-folder-correction = Informe uma pasta para escanear.
projects-import-id-correction = Use de 2 a 4 letras maiúsculas.
projects-import-title-correction = Informe um título para o projeto.
projects-comparison-branch-correction = Informe o nome de uma branch local, como main ou release/next.

## Failure reasons

failure-unexpected = O servidor não conseguiu concluir esta ação. Os detalhes estão no log do gtl-server.
failure-unavailable = Um serviço necessário está temporariamente indisponível. Tente novamente.
failure-busy = O servidor está ocupado. Tente novamente em instantes.
failure-changed = O estado mudou enquanto esta ação era executada. Tente novamente.
failure-gone-push-operation = Esta operação de push não está mais disponível.
failure-gone-viewer-tab = Esta aba do visualizador não está mais disponível.
failure-gone-commit = Este commit não está mais disponível.
failure-gone-diff-file = Este arquivo do diff não está mais disponível.
failure-gone-source-range = Este trecho do código-fonte não está mais disponível.
failure-gone-snapshot = Este snapshot não está mais disponível.
failure-gone-project = Este projeto não está mais disponível.
failure-invalid-request = A requisição tem um valor inválido em `{ $field }`.
failure-unrecognized = O servidor informou uma falha { $class } que esta versão não reconhece.
failure-push-nothing-to-push = Não há commits sem push até este SHA.
failure-push-no-upstream = A branch { $branch } não tem upstream remoto. Configure o upstream antes de fazer push.
failure-push-detached = Faça checkout de uma branch antes de fazer push.
failure-push-checkout-changed = O checkout mudou para { $current }. Revise o push novamente.
failure-push-commit-removed = O commit { $commit } não está mais no histórico desta branch. Selecione um commit atual ou abra um novo diff.
failure-push-destination-changed = O destino do upstream mudou. Revise o push novamente.
failure-push-multiple-destinations = O remote { $remote } precisa ter exatamente uma URL de push para um push atômico.
failure-push-review-expired = Esta revisão de push expirou. Revise o push novamente.
failure-push-history-full = O servidor já acompanha { $operations } operações de push. Tente novamente depois que um push terminar.
failure-push-remote-ahead = O remote tem commits que esta branch não tem. Faça pull deles antes do push.
failure-push-remote-rejected-message = O remote rejeitou o push: { $message }
failure-push-remote-rejected = O remote rejeitou o push.
failure-push-git-failed = O Git não conseguiu concluir o push.
failure-settings-invalid = As configurações do usuário em { $path } são inválidas. Corrija o arquivo de configurações ou faça backup dele e redefina-o.
failure-settings-stale = As configurações mudaram desde que foram carregadas. Recarregue-as antes de salvar de novo.
failure-settings-locked =
    { $seconds ->
        [one] Outro editor manteve o bloqueio das configurações por { $seconds } segundo. Tente novamente.
       *[other] Outro editor manteve o bloqueio das configurações por { $seconds } segundos. Tente novamente.
    }
failure-settings-path-unavailable = O caminho da configuração do usuário está indisponível.
failure-viewer-source-preparing = A fonte do diff ainda está sendo preparada. Tente novamente em instantes.
failure-viewer-reveal-too-large = Os arquivos revelados excedem o limite do cache do visualizador.
failure-viewer-snapshot-name-invalid = Nomes de snapshot precisam ter de 1 a { $characters } caracteres em uma única linha.
failure-viewer-snapshot-pending = Aguarde o snapshot terminar de salvar e tente novamente.
failure-viewer-modified-files-unavailable = Os arquivos modificados ficam indisponíveis enquanto uma seleção de commit está pendente.
failure-viewer-range-too-large = Esta seção tem texto demais para exibir.
failure-viewer-search-too-large = A busca encontrou resultados demais. Refine a consulta.
failure-viewer-response-too-large = Esta visualização é grande demais para enviar ao visualizador.
failure-viewer-file-not-in-diff = O arquivo não está no diff atual.
failure-viewer-file-deleted = Arquivos excluídos não podem ser abertos.
failure-viewer-file-unavailable = O arquivo está ausente ou ilegível na working tree.
failure-viewer-file-outside-repository = O arquivo aponta para fora do repositório.
failure-viewer-editor-failed = O editor configurado não conseguiu abrir o arquivo.
failure-viewer-source-directory-missing = O diretório do repositório { $path } não foi encontrado. Abas live voltam a atualizar quando ele for restaurado.
failure-viewer-source-not-repository = { $path } não é um repositório Git. Abas live voltam a atualizar quando o repositório for restaurado.
failure-viewer-source-unavailable = O repositório está indisponível. Abas live voltam a atualizar quando ele for restaurado.
failure-viewer-render-failed = Não foi possível renderizar o diff. Tente novamente.
failure-viewer-commit-failed = Não foi possível renderizar o commit selecionado. Mostre todas as alterações e tente novamente.
failure-viewer-row-too-large = Uma linha do diff é grande demais para exibir.
failure-project-catalogue-unavailable = O catálogo de projetos está indisponível. Verifique o servidor do GTL e tente novamente.
failure-project-already-exists = Já existe um projeto com este ID, título ou origem.
failure-project-catalogue-full = O catálogo de projetos já tem { $projects } projetos.
failure-project-scan-failed = Não foi possível escanear esta pasta. Verifique se ela existe e pode ser lida.
failure-project-scan-folder-not-absolute = Informe um caminho de pasta absoluto ou que comece com ~/.
failure-project-scan-folder-not-directory = O caminho a escanear não é uma pasta.
failure-project-scan-folder-not-utf8 = O caminho da pasta a escanear não é UTF-8 válido.
failure-project-home-unavailable = A pasta pessoal está indisponível.
failure-project-too-many-repositories = Esta pasta contém mais de { $repositories } repositórios. Escolha uma pasta mais específica.
failure-project-comparison-branch-missing = A branch local de comparação '{ $branch }' não existe em { $path }.
failure-project-repository-unborn = O repositório em { $path } não tem commits para comparar.
failure-project-no-common-ancestor = A branch local de comparação '{ $branch }' e o HEAD não têm ancestral comum em { $path }.
failure-project-scan-stale = Esta pasta mudou desde o escaneamento. Escaneie novamente.
failure-project-already-managed = Este repositório já é um projeto gerenciado.
failure-project-commit-count-unavailable = O Git não conseguiu contar os commits à frente da branch de comparação.
failure-repository-not-a-repository = { $path } não está dentro de um repositório Git.
failure-repository-no-repositories = Nenhum repositório Git foi encontrado em { $root }.
failure-repository-search-failed = Não foi possível procurar repositórios em { $path }.

## Viewer client errors

client-error-protocol-mismatch = Este visualizador desktop e o gtl-server usam versões diferentes. Atualize e reinicie os dois.
client-error-disconnected = O visualizador desktop está temporariamente indisponível.
client-error-invalid-message = O visualizador não conseguiu ler esta resposta. Atualize e tente novamente.
client-error-stream-closed = O fluxo do visualizador foi fechado. Atualize para carregá-lo de novo.
client-error-desktop = A janela do desktop não conseguiu concluir esta ação.

## Notifications

toast-viewport = Notificações
toast-dismiss = Dispensar notificação
toast-waiting =
    { $count ->
        [one] Mais { $count } notificação
       *[other] Mais { $count } notificações
    }

## Settings recovery

settings-recovery-label = Recuperação das configurações
settings-recovery-title = As configurações do usuário são inválidas
settings-recovery-message = Corrija o arquivo e tente de novo, ou restaure os padrões. A redefinição salva o arquivo original como config.yyyymmdd-hhmmss-backup.toml antes de substituí-lo. Os horários do backup usam UTC.
settings-recovery-loading = Carregando os detalhes das configurações...
settings-recovery-resetting = Fazendo backup e redefinindo...
settings-recovery-reset = Fazer backup e redefinir configurações
settings-recovery-valid = O arquivo de configurações agora é válido. Tente de novo para continuar.
settings-recovery-reset-done = Configurações redefinidas. Backup: { $path }

## Viewer navigation

viewer-settings = Configurações

## Push

push-button = Fazer push
push-check-result = Verificar resultado
push-check-result-title = Verificar o resultado do push: { $message }
push-waiting = Aguardando para fazer push dos commits...
push-running = Fazendo push dos commits...
push-completed = Push concluído.
push-not-started = O push não começou. Revise-o novamente.
push-status-unreadable = { $error } O push pode ainda estar em andamento. Use o botão de push para verificar o resultado.
push-status-pending = O resultado do push ainda não está disponível. Use o botão de push para verificar o resultado.
push-availability-nothing = Não há commits sem push até este SHA
push-dialog-title =
    { $count ->
        [one] Fazer push de { $count } commit?
       *[other] Fazer push de { $count } commits?
    }
push-dialog-description = Os commits até { $commit } serão enviados. Commits mais novos continuam locais.
push-details-disclosure = Detalhes e comando
push-confirm-shortcut = Confirmar push (Enter)
push-detail-destination = Destino
push-detail-remote-url = URL remota
push-detail-project = Projeto
push-detail-directory = Diretório
push-detail-branch = Branch
push-detail-remote-branch = Branch remota
push-detail-commits = Commits para push
push-detail-remote = Remoto
push-detail-selected-sha = SHA selecionado
push-detail-command = Comando
push-command-help = Explicar comando
push-command-copy = Copiar comando
push-command-copied = Comando copiado
push-command-copy-failed = Não foi possível copiar o comando
push-command-arg-directory = Executa o Git neste diretório.
push-command-arg-mirror = Limita o push à branch selecionada, mesmo se o remoto estiver configurado para espelhamento.
push-command-arg-push = Envia os commits ao remoto.
push-command-arg-atomic = Exige que o remoto atualize todas as referências solicitadas juntas.
push-command-arg-porcelain = Fornece resultados estruturados para o Git Tools identificar referências rejeitadas.
push-command-arg-tags = Mantém as tags relacionadas locais.
push-command-arg-submodules = Não faz push de submódulos.
push-command-arg-separator = Encerra as opções; o destino vem em seguida.
push-command-arg-remote = Indica o remoto que recebe os commits.
push-command-arg-ref = Envia o commit selecionado para a branch remota indicada.

## Project status

projects-issue-request-failed = Status do Git indisponível
projects-issue-repository-absent = Repositório não encontrado
projects-issue-head-unavailable = Status da branch indisponível
projects-issue-working-tree-unavailable = Status da working tree indisponível
projects-issue-upstream-missing = Nenhum upstream configurado
projects-local-modified-untracked = Arquivos modificados e não rastreados
projects-local-modified = Arquivos modificados
projects-local-untracked = Arquivos não rastreados
projects-local-clean = Working tree limpa
projects-ahead-upstream =
    { $count ->
        [0] Nada para enviar
        [one] { $count } commit sem push
       *[other] { $count } commits sem push
    }
projects-ahead-branch =
    { $count ->
        [0] Nada à frente de { $base }
        [one] { $count } commit à frente de { $base }
       *[other] { $count } commits à frente de { $base }
    }
projects-status-loading = Carregando o status do Git
projects-detached-head = HEAD destacado
projects-review-pending = Alterações para revisar
projects-review-clean = Em dia
projects-review-comparison-unavailable = Comparação indisponível
projects-review-status-unavailable = Status indisponível
projects-review-stale = O status do Git está desatualizado. Os últimos valores obtidos estão sendo exibidos. Tente buscar o status do Git de novo.
projects-retry-status-for = Buscar de novo o status do Git de { $project }
projects-retry-status = Buscar de novo o status do Git
projects-comparison-branch = Branch de comparação
projects-change-comparison-branch = Alterar a branch de comparação

## Projects table

projects-table-caption = Projetos gerenciados
projects-table-status = Status
projects-table-project = Projeto
projects-table-branch = Branch
projects-table-changes = Alterações
projects-table-actions = Ações
projects-local-counts =
    { $tracked ->
        [0] 0 arquivos rastreados alterados
        [one] { $tracked } arquivo rastreado alterado
       *[other] { $tracked } arquivos rastreados alterados
    }; { $untracked ->
        [0] 0 arquivos não rastreados
        [one] { $untracked } arquivo não rastreado
       *[other] { $untracked } arquivos não rastreados
    }
projects-tracked-changes = Alterações rastreadas
projects-no-changes = Nenhuma alteração pendente
projects-changes-popover = Alterações do projeto

## Projects dashboard

projects-add = Adicionar projetos
projects-diff-all = Diff dos projetos não enviados
projects-diff-all-empty = Nenhum projeto tem commits não enviados.
projects-diff-all-opened =
    { $count ->
        [one] Snapshot de diff aberto para { $count } projeto.
       *[other] Snapshots de diff abertos para { $count } projetos.
    }
projects-diff-all-warning =
    { $count ->
        [one] { $count } projeto não pôde ser comparado: { $projects }.
       *[other] { $count } projetos não puderam ser comparados: { $projects }.
    }
projects-snapshots-title = Snapshots
projects-unavailable = Projetos indisponíveis
projects-unavailable-message = Verifique o catálogo de projetos e tente novamente.
projects-loading = Carregando projetos
projects-empty = Nenhum projeto gerenciado
projects-empty-message = Os projetos gerenciados no Git Tools aparecem aqui.
projects-empty-active = Nenhum projeto ativo
projects-empty-active-message = Adicione projetos ou escolha Pausados para encontrar os projetos pausados.
projects-empty-paused = Nenhum projeto pausado
projects-empty-paused-message = Os projetos pausados aparecem aqui.
projects-status-filter-label = Filtrar projetos por status
projects-status-active = Ativos
projects-status-paused = Pausados
projects-status-all = Todos
projects-count =
    { $count ->
        [one] { $count } projeto
       *[other] { $count } projetos
    }
projects-paused-status = Pausado
projects-pause = Pausar { $project }
projects-resume = Retomar { $project }
projects-paused = { $project } pausado.
projects-resumed = { $project } retomado.
projects-edit = Editar { $project }
projects-edit-short = Editar projeto
projects-edit-title = Editar { $project }
projects-edit-save = Salvar
projects-edit-discard-title = Descartar alterações não salvas?
projects-edit-discard-description = Suas alterações em { $project } não foram salvas.
projects-edit-discard = Descartar alterações
projects-snapshot-history = Histórico de snapshots
projects-per-page = Por página
projects-per-page-label = Projetos por página
projects-all-snapshots = Todos os snapshots
projects-comparison-branch-value = Branch de comparação: { $branch }
projects-comparison-branch-hint = Usada quando a branch atual não tem upstream.
projects-comparison-save = Salvar comparação
projects-import-folder = Pasta a escanear
projects-import-folder-placeholder = ~/meus-projetos ou /caminho/para/projetos
projects-import-choose-folder = Escolher pasta
projects-import-scan = Escanear
projects-import-scanning = Escaneando pastas…
projects-import-found =
    { $found ->
        [one] { $found } repositório encontrado
       *[other] { $found } repositórios encontrados
    } · { $selected ->
        [0] 0 selecionados
        [one] { $selected } selecionado
       *[other] { $selected } selecionados
    }
projects-import-deselect-all = Desmarcar todos
projects-import-select-all = Selecionar todos
projects-import-none-found = Nenhum repositório Git foi encontrado nesta pasta.
projects-import-empty = Escolha uma pasta para encontrar repositórios Git. Os resultados começam desmarcados.
projects-import-footer = Só os repositórios selecionados são adicionados. Cada resultado é informado separadamente.
projects-import-add =
    { $selected ->
        [0] Adicionar 0 selecionados
        [one] Adicionar { $selected } selecionado
       *[other] Adicionar { $selected } selecionados
    }
projects-import-created = Criado
projects-import-restored = Restaurado
projects-import-failed = Falhou
projects-import-new = Novo
projects-import-active = Projeto ativo
projects-import-paused = Projeto pausado
projects-import-unmanaged = Não gerenciado · Restaurar
projects-import-select-row = Selecionar { $path }
projects-import-id-label = ID do projeto { $project }
projects-import-title-label = Título do projeto { $project }

## Recipe labels

recipe-label-unpushed = { $repository }: diff
recipe-label-unpushed-commits =
    { $count ->
        [0] { $repository }: nenhum commit
        [one] { $repository }: { $count } commit
       *[other] { $repository }: { $count } commits
    }
recipe-label-working-tree = { $repository }: { $base }->working tree
recipe-label-commit = { $repository }: { $rev }^!
recipe-label-compared = { $repository } | { $base }->{ $head }
recipe-label-working-tree-head = working tree
recipe-label-range = { $repository }: { $range }
recipe-label-merge-into = { $repository }: merge ->{ $base }
recipe-label-merge = { $repository }: merge { $branch }->{ $upstream }
recipe-label-last-commits =
    { $count ->
        [one] { $repository }: último commit
       *[other] { $repository }: últimos { $count } commits
    }

## Tabs

tab-snapshot-name = Nome do snapshot
tab-sortable = aba reordenável
tab-unpin-named = Desafixar { $tab }
tab-unpin = Desafixar aba
tab-pin = Fixar aba
tab-close-named = Fechar { $tab }
tab-close = Fechar aba
tab-close-others = Fechar as outras
tab-rename-snapshot = Renomear snapshot
tab-actions = Ações da aba
tab-live = Live
tab-state-ready = Pronto
tab-state-rendering = Renderizando
tab-state-stopped = Renderização interrompida
tab-state-failed = Falha na renderização
tabs-open-count =
    { $count ->
        [0] Nenhum diff aberto
        [one] { $count } diff aberto
       *[other] { $count } diffs abertos
    }

## Shared controls

dialog-close = Fechar diálogo
dialog-cancel = Cancelar
dialog-close-named = Fechar { $title }
dialog-close-short = Fechar
no-data = Sem dados
pagination-position = Posição da página em { $label }
pagination-pages = Páginas de { $label }
pagination-first = Primeira página
pagination-previous = Página anterior
pagination-next = Próxima página
pagination-last = Última página
pagination-page-of = Página { $number } de { $count }
scrollbar-diff-horizontal = Rolar o diff horizontalmente
scrollbar-horizontal = Rolar horizontalmente
scrollbar-vertical = Rolar verticalmente
table-sort-descending = Ordenar { $column } em ordem decrescente
table-sort-ascending = Ordenar { $column } em ordem crescente
table-sort-by = Ordenar por { $column }
inline-editor-hint = Enter para salvar, Escape para cancelar
inline-editor-saving = Salvando o nome
extensions-none = Nenhuma extensão selecionada
extensions-add-extension = Adicionar extensão…
extensions-remove = Remover .{ $extension }
extensions-search = Busque ou adicione uma extensão
extensions-search-placeholder = Buscar ou adicionar…
extensions-clear-search = Limpar a busca de extensões
extensions-options = Extensões para filtrar
extensions-in-diff = Neste diff
extensions-other = Outras extensões
extensions-more = Busque para encontrar mais extensões
extensions-toggle-named = Alternar .{ $extension }
extensions-add = Adicionar .{ $extension }
extensions-invalid = Informe uma extensão de arquivo final, como .lock ou .md.

## Diff document

diff-view-title-diff = diff
diff-view-title-merge-diff = merge-diff
diff-view-title-commit = commit { $commit }
file-status-added = Arquivo adicionado
file-status-deleted = Arquivo excluído
file-status-renamed = Arquivo renomeado
file-status-modified = Arquivo modificado
diff-line-omitted =
    { $count ->
        [one] ... (+{ $count } caractere omitido)
       *[other] ... (+{ $count } caracteres omitidos)
    }
diff-search-all-files-label = Buscar código em todos os arquivos
diff-search-all-files-placeholder = Buscar código em todos os arquivos...
diff-search-code = Buscar código
diff-search-previous = Resultado anterior
diff-search-previous-title = Resultado anterior (Shift+Enter)
diff-search-next = Próximo resultado
diff-search-next-title = Próximo resultado (Enter)
diff-search-close = Fechar busca
diff-search-close-title = Fechar busca (Escape)
diff-find-query-too-long = A busca é limitada a { $bytes } bytes UTF-8.
diff-find-searching = Buscando…
diff-find-no-matches = Nenhum resultado
diff-find-matches =
    { $count ->
        [one] { $count } resultado
       *[other] { $count } resultados
    }
diff-find-matches-wrapped =
    { $count ->
        [one] { $count } resultado · voltou ao início
       *[other] { $count } resultados · voltou ao início
    }
diff-rendered = Diff renderizado
diff-rendered-for = Diff renderizado de { $title }
diff-rows-label =
    Linhas do diff { $layout ->
        [split] lado a lado
       *[unified] unificado
    }, { $density ->
        [full] arquivo completo
       *[compact] só alterações
    }
diff-preparing = Preparando o diff…
diff-source-unavailable = A fonte do diff está indisponível
diff-source-unavailable-message = Atualize esta aba para tentar carregar o diff de novo.
diff-refresh = Atualizar
diff-open-in-editor = Abrir no editor de texto
copy-file-path = Copiar o caminho do arquivo
copy-copied = Copiado
copy-failed = Falhou
copy-relative-path = Caminho relativo
copy-absolute-path = Caminho absoluto
copy-relative-path-action = Copiar o caminho relativo
copy-absolute-path-action = Copiar o caminho absoluto
copy-context = Copiado com contexto
copy-context-lines = Copiado com contexto - linhas { $lines }
copy-context-files =
    { $count ->
        [one] Copiado com contexto - { $count } arquivo
       *[other] Copiado com contexto - { $count } arquivos
    }
copy-selection-failed = Não foi possível copiar o texto selecionado.
copy-selection-empty = A seleção não contém linhas de código.
copy-selection-pending = Copiando o código selecionado…

## Diff rows

diff-rows-disconnected = O fluxo de linhas do diff foi desconectado.
diff-rows-invalid = O servidor retornou linhas de diff inválidas. Tente carregar esta visualização de novo.

## Snapshot history

history-kind-diff = Diff
history-kind-merge-diff = Diff de merge
history-filter-all = Todos os projetos
history-filter-unassociated = Sem projeto
history-project = Projeto
history-project-label = Projeto do snapshot
history-renders = Diffs renderizados recentemente
history-unavailable = Os snapshots estão indisponíveis
history-empty = Nenhum snapshot ainda
history-empty-message = Crie um snapshot para salvar a comparação deste projeto.
history-column-id = ID
history-column-diff = Diff
history-column-kind = Tipo
history-column-range = Intervalo
history-column-rendered = Renderizado em
history-loading = Carregando o histórico
history-open = Abrir
history-open-named = Abrir { $title }
history-copy-json = Copiar o JSON da renderização
history-copy-json-named = Copiar o JSON de { $title }
history-label = Histórico
history-render-count =
    { $count ->
        [0] Nenhuma renderização
        [one] { $count } renderização
       *[other] { $count } renderizações
    }

## Diff workspace

workspace-heading = Visualizador de diffs
workspace-panels = Painéis do visualizador
workspace-files = Arquivos
workspace-changed-files = Arquivos alterados
workspace-commits = Commits
workspace-unavailable = O estado do visualizador está indisponível
workspace-loading = Carregando o visualizador
workspace-active-diff = Diff ativo
workspace-empty = Nenhum diff aberto
workspace-empty-message = Execute um comando de diff do git-tools ou abra um projeto para ver um diff.
workspace-source-unavailable = Repositório indisponível
workspace-no-changes = Nenhuma alteração
workspace-no-changes-message = Nenhuma alteração nesta comparação. Atualize a aba para verificar de novo.
workspace-no-changes-live-message = Nenhuma alteração nesta comparação. As atualizações aparecem automaticamente quando o HEAD muda.
workspace-live-warnings = Avisos de atualização do diff live
workspace-live = Live
workspace-live-start-hint = Atualizar esta aba sempre que o repositório mudar
workspace-live-stop-hint = Parar de atualizar esta aba e manter o snapshot atual
workspace-refresh = Atualizar
workspace-refresh-hint = Mostrar as alterações mais recentes do intervalo desta aba
workspace-live-recent-errors = Erros recentes de atualização
workspace-live-occurrences =
    { $count ->
        [one] Ocorreu { $count } vez
       *[other] Ocorreu { $count } vezes
    }
workspace-modified-files = Arquivos modificados
workspace-modified-files-hint = Inspecionar as alterações atuais em stage, fora de stage e não rastreadas em relação ao HEAD
commits-empty = Nenhum commit
commits-loading = Carregando commits...
commits-load-more = Carregar mais
commits-select = Selecionar o commit { $commit }: { $subject }
commits-merge = merge
commits-details-for = Detalhes do commit { $commit }
commits-details = Detalhes do commit
commits-date = Data
commits-id = ID do commit
commits-copy-id = Copiar o ID do commit

## Diff workspace chrome

files-empty = Nenhum arquivo alterado
files-count =
    { $count ->
        [0] 0 arquivos
        [one] { $count } arquivo
       *[other] { $count } arquivos
    }
files-sort = Ordenar arquivos
files-sort-selected =
    { $sort ->
        [changes] Ordenar arquivos: mais alterados
       *[path] Ordenar arquivos: caminho
    }
files-sort-path = Caminho
files-sort-changes = Mais alterados
sidebar-visibility = Visibilidade das barras laterais
sidebar-toggle-files = Alternar a barra lateral de arquivos
sidebar-toggle-commits = Alternar a barra lateral de commits

## Path filter

path-filter-label = Encontrar um arquivo pelo caminho
path-filter-results = Arquivos correspondentes
path-filter-empty = Nenhum arquivo corresponde
path-filter-searching = Buscando arquivos...

## Diff file filters

file-filters-label = Filtrar arquivos
file-filters-label-hidden =
    { $count ->
        [one] Filtrar arquivos: { $count } arquivo oculto
       *[other] Filtrar arquivos: { $count } arquivos ocultos
    }
file-filters-text = Texto nos arquivos
file-filters-text-placeholder = Buscar código…
file-filters-changes = Alterações
file-filters-change-added = Adicionados
file-filters-change-removed = Removidos
file-filters-change-modified = Modificados
file-filters-change-modified-hint = Inclui arquivos renomeados
file-filters-since = Alterado desde
file-filters-since-notice = Desde
file-filters-since-any-time = Qualquer momento
file-filters-since-last-hour = Última hora
file-filters-since-today = Hoje
file-filters-since-last-24-hours = Últimas 24 horas
file-filters-since-last-7-days = Últimos 7 dias
file-filters-since-custom = Personalizado…
file-filters-since-custom-label = Data e hora de início das alterações
file-filters-since-description = Mostra só as alterações com commit depois deste momento. Alterações sem commit continuam visíveis.
file-filters-since-unavailable = Arquivos modificados mostram só alterações sem commit.
file-filters-extensions = Extensões
file-filters-hidden-count =
    { $count ->
        [0] Nenhum arquivo oculto
        [one] { $count } arquivo oculto
       *[other] { $count } arquivos ocultos
    }
file-filters-clear = Limpar os filtros
file-filters-no-matches = Nenhum arquivo corresponde aos filtros
file-filters-no-matches-message = Altere ou limpe os filtros para mostrar os arquivos ocultos.
extensions-selected = Extensões filtradas
extensions-mode-label = Modo do filtro
extensions-mode-only = Mostrar só
extensions-mode-hide = Ocultar
extensions-mode-only-description = Mostra só os arquivos alterados com estas extensões.
extensions-mode-hide-description = Oculta os arquivos alterados com estas extensões.
extensions-too-many-changes = Há alterações de filtro pendentes demais. Tente novamente em instantes.

## Project comparisons

project-diff-failed = Não foi possível abrir a comparação
project-diff-opening = Abrindo o modo
project-diff-loading = Carregando o diff...

## Offline artifacts


## Desktop host

tray-show = Mostrar
tray-quit = Sair
projects-import-picker-title = Escolha uma pasta para escanear

files-expand-diffs = Expandir todos os diffs
files-collapse-diffs = Recolher todos os diffs
tab-details = Detalhes da comparação
tab-details-base = Base
tab-details-head = Head
tab-push-activate = Abra esta aba para revisar um push.
settings-back = Voltar
settings-appearance = Aparência
settings-locale = Idioma e datas
settings-snapshots = Aba de diff
settings-git = Git
settings-autosave = As alterações são salvas automaticamente

projects-viewer-push-no-confirmation = Sem confirmação de push no viewer
projects-viewer-push-no-confirmation-hint = Quando ativado, o push é imediato. Desativado por padrão para evitar pushes acidentais.
review-actions-label = Ações de revisão do diff
review-push = Push
review-push-check = Verificar
review-close-label = Fechar
review-unpushed-hint = Este snapshot contém commits que ainda não receberam push.
review-close = Fechar diff
review-close-pinned = Desafixe este diff antes de fechá-lo.
review-previous-diff = Diff anterior
review-next-diff = Próximo diff
review-actions-hide = Ocultar ações de revisão
review-actions-show = Mostrar ações de revisão

commit-search-label = Buscar commits
commit-search-placeholder = Buscar…
commit-search-hint = Mensagem ou hash
commit-search-time-from = De
commit-search-time-until = Até
commit-search-time-hint = Horário local; cada limite é opcional.
commit-search-time-clear = Limpar filtro de tempo
commit-search-time-invalid = Informe uma data e hora local válida.
commit-search-time-reversed = Até deve ser igual ou posterior a De.
commit-search-snapshot = Snapshot
commit-search-branch = Branch
commit-search-scope = Escopo da busca
commit-search-snapshot-option = Este snapshot
commit-search-branch-option = Branch ativa
commit-search-close = Fechar busca de commits
commit-search-clear = Limpar busca de commits
commit-search-snapshot-hint = Apenas commits deste snapshot.
commit-search-branch-hint = Commits locais da branch ativa.
commit-search-project-title = Buscar commits · { $project }
commit-search-loading = Buscando commits…
commit-search-count =
    { $total ->
        [0] Nenhum resultado
        [one] 1 resultado
       *[other] { $total } resultados
    }
commit-search-count-limited = { $shown } de { $total } resultados
commit-search-results = Commits encontrados
commit-search-open = Abrir commit { $id }: { $subject }
commit-search-empty = Tente uma busca mais curta, um hash de commit ou um intervalo de tempo maior.
commit-search-detached = Faça checkout de uma branch para buscar seus commits.

settings-keybindings = Atalhos de teclado
keybindings-description = Encontre um comando ou atalho. Selecione um atalho para alterá-lo ou clique duas vezes em uma linha.
keybindings-search = Pesquisar atalhos de teclado
keybindings-search-placeholder = Pesquisar comandos ou atalhos
keybindings-record-search = Pressione um atalho para encontrar seu comando. Escape encerra a gravação.
keybindings-record = Gravar teclas para pesquisar
keybindings-clear-search = Limpar pesquisa
keybindings-reset-all = Restaurar todos os atalhos
keybindings-modified = Mostrar apenas modificados
keybindings-command = Comando
keybindings-shortcut = Atalho
keybindings-when = Quando
keybindings-source = Origem
keybindings-actions = Ações
keybindings-context-diff = Área de diffs
keybindings-source-user = Usuário
keybindings-source-default = Padrão
keybindings-empty = Nenhum atalho corresponde à sua pesquisa.
keybindings-edit = Alterar atalho de { $command }
keybindings-remove = Remover atalho de { $command }
keybindings-reset = Restaurar atalho de { $command }
keybindings-unassigned = Sem atalho
keybindings-recorder-title = Alterar atalho
keybindings-recorder-instructions = Pressione o atalho desejado e depois Enter para salvar. Escape cancela. Tab move entre os controles.
keybindings-recorder-waiting = Pressione as teclas
keybindings-current = Atual:
keybindings-replace-hint = Substituir este atalho o remove do outro comando.
keybindings-replace = Substituir atalho
keybindings-save = Salvar
keybindings-cancel = Cancelar
keybindings-search-files = Arquivos: Encontrar arquivo
keybindings-search-text = Diff: Encontrar texto
keybindings-toggle-files = Exibir: Alternar painel de arquivos
keybindings-toggle-commits = Exibir: Alternar painel de commits
keybindings-push = Git: Push do diff revisado
keybindings-next-tab = Abas: Próximo diff
keybindings-previous-tab = Abas: Diff anterior
keybindings-close-tab = Abas: Fechar diff
keybindings-pin-tab = Abas: Fixar ou desafixar diff
keybindings-close-others = Abas: Fechar outros diffs
keybindings-conflict = “{ $first }” e “{ $second }” usam o mesmo atalho.
keybindings-ambiguous = Use cada modificador apenas uma vez no atalho.
keybindings-modifier-required = Inclua Ctrl, Alt, Command ou Super no atalho.
keybindings-unsupported = Use uma letra, número, F1–F12 ou tecla de navegação com um modificador.
keybindings-count = { $count ->
    [0] Nenhum atalho
    [one] 1 atalho
   *[other] { $count } atalhos
    }
keybindings-context-tabs = Abas do visualizador
