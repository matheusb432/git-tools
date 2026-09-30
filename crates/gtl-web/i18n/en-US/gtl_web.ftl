# Viewer copy for en-US, the fallback language. `fl!` checks every message ID
# and argument against this file at compile time. Keep Git and developer
# jargon (commit, push, branch, diff, snapshot, upstream, HEAD, live) untranslated.

## Shared actions

action-retry = Retry
action-try-again = Try again

## Dates

date-just-now = just now
date-minutes-ago =
    { $count ->
        [one] { $count } minute ago
       *[other] { $count } minutes ago
    }
date-hours-ago =
    { $count ->
        [one] { $count } hour ago
       *[other] { $count } hours ago
    }
date-days-ago =
    { $count ->
        [one] { $count } day ago
       *[other] { $count } days ago
    }

## Document titles

document-title-projects = Projects - git-tools
document-title-settings = Settings - git-tools
document-title-viewer = Viewer - git-tools

## Application navigation and window chrome

navigation-label = Viewer navigation
navigation-projects = Projects
navigation-open-diffs = Open diffs
navigation-no-open-diffs = No open diffs
window-drag-region = Drag to move window
window-controls = Window controls
window-minimize = Minimize window
window-maximize = Maximize window
window-restore = Restore window
window-close = Close window

## Server connection

connection-connecting = Connecting to the viewer server…
connection-retrying = { $message } Retrying automatically.
connection-try-now = Try now
feedback-snapshots-skipped-named =
    { $count ->
        [one] Skipped { $count } diff with no commits or changed files: { $labels }.
       *[other] Skipped { $count } diffs with no commits or changed files: { $labels }.
    }

## User settings page

settings-title = User settings
settings-loading = Loading settings
settings-unavailable = Settings are unavailable
settings-configuration-file = Configuration file
settings-edit-stale = Settings changed since this page loaded. Reload them before saving again.
settings-edit-field-rejected = Correct the highlighted setting and retry.
settings-edit-rejected = One or more settings were rejected. Reload the saved values and try again.
settings-edit-invalid-file = The settings file became invalid. Reload it to repair or reset it.

## Viewer settings form

settings-language = Language
settings-date-format = Date format
settings-date-format-hint = Dates use your time zone. Saved HTML diffs keep ISO dates.
settings-date-format-iso = ISO ({ $sample })
settings-date-format-day-first = Day first ({ $sample })
settings-date-format-month-first = Month first ({ $sample })
settings-date-format-relative = Relative ({ $sample })
settings-ui-scale = Interface size
settings-reduce-motion = Reduced motion
settings-reduce-motion-hint = Always reduce animations. When unchecked, follow your system preference.
settings-theme = Theme
settings-theme-default = Built-in default (Dark)
settings-layout = Layout
settings-layout-unified = Unified
settings-layout-split = Side by side
settings-wrap-lines = Wrap lines
settings-wrap-lines-hint = Fit source lines to the available width.
settings-copy-with-line-context = Copy with line context
settings-copy-with-line-context-hint = Include the file path and line numbers when copying source from a diff.
settings-density = View
settings-density-compact = Changes only
settings-density-full = Full file
settings-focus-window = Focus window when opening a diff
settings-focus-window-hint = Bring the desktop window forward when a command opens a diff.
settings-push-confirmation = No confirmation on push in the CLI
settings-push-confirmation-hint = Push immediately from the CLI. Off by default to prevent accidental pushes.
settings-field-correction = Unsupported value. Choose another option.
settings-reload = Reload settings

## Projects

projects-import-folder-correction = Enter a folder to scan.
projects-import-id-correction = Use 2 to 4 uppercase letters.
projects-import-title-correction = Enter a project title.
projects-comparison-branch-correction = Enter a local branch name, such as main or release/next.

## Failure reasons

failure-unexpected = The server could not complete this action. Details are in the gtl-server log.
failure-unavailable = A required service is temporarily unavailable. Try again.
failure-busy = The server is busy. Try again shortly.
failure-changed = State changed while this action was running. Try again.
failure-gone-push-operation = This push operation is no longer available.
failure-gone-viewer-tab = This viewer tab is no longer available.
failure-gone-commit = This commit is no longer available.
failure-gone-diff-file = This diff file is no longer available.
failure-gone-source-range = This source range is no longer available.
failure-gone-snapshot = This snapshot is no longer available.
failure-gone-project = This project is no longer available.
failure-invalid-request = The request has an invalid `{ $field }` value.
failure-unrecognized = The server reported a { $class } failure that this version does not recognize.
failure-push-nothing-to-push = There are no unpushed commits through this SHA.
failure-push-no-upstream = Branch { $branch } has no remote upstream. Configure its upstream before pushing.
failure-push-detached = Check out a branch before pushing.
failure-push-checkout-changed = The checkout changed to { $current }. Review the push again.
failure-push-commit-removed = Commit { $commit } is no longer in this branch's history. Select a current commit or open a new diff.
failure-push-destination-changed = The upstream destination changed. Review the push again.
failure-push-multiple-destinations = Remote { $remote } must have exactly one push URL for an atomic push.
failure-push-review-expired = This push review expired. Review the push again.
failure-push-history-full = The server already tracks { $operations } push operations. Try again after a push completes.
failure-push-remote-ahead = The remote has commits that this branch does not. Pull them before pushing.
failure-push-remote-rejected-message = The remote rejected the push: { $message }
failure-push-remote-rejected = The remote rejected the push.
failure-push-git-failed = Git could not complete the push.
failure-settings-invalid = User settings at { $path } are invalid. Repair the settings file or back it up and reset it.
failure-settings-stale = Settings changed since they were loaded. Reload them before saving again.
failure-settings-locked =
    { $seconds ->
        [one] Another editor held the settings lock for { $seconds } second. Try again.
       *[other] Another editor held the settings lock for { $seconds } seconds. Try again.
    }
failure-settings-path-unavailable = The user configuration path is unavailable.
failure-viewer-source-preparing = The diff source is still being prepared. Try again shortly.
failure-viewer-reveal-too-large = The revealed files exceed the viewer cache limit.
failure-viewer-snapshot-name-invalid = Snapshot names need 1 to { $characters } characters on a single line.
failure-viewer-snapshot-pending = Wait for the snapshot to finish saving, then try again.
failure-viewer-modified-files-unavailable = Modified files are unavailable while a commit selection is pending.
failure-viewer-range-too-large = This section contains too much text to display.
failure-viewer-search-too-large = The search matched too much. Narrow the query.
failure-viewer-response-too-large = This view is too large to send to the viewer.
failure-viewer-file-not-in-diff = The file is not in the current diff.
failure-viewer-file-deleted = Deleted files cannot be opened.
failure-viewer-file-unavailable = The file is missing or unreadable in the working tree.
failure-viewer-file-outside-repository = The file resolves outside its repository.
failure-viewer-editor-failed = The configured editor could not open the file.
failure-viewer-source-directory-missing = The repository directory { $path } was not found. Live tabs update again when it is restored.
failure-viewer-source-not-repository = { $path } is not a Git repository. Live tabs update again when the repository is restored.
failure-viewer-source-unavailable = The repository is unavailable. Live tabs update again when it is restored.
failure-viewer-render-failed = The diff could not be rendered. Please retry.
failure-viewer-commit-failed = The selected commit could not be rendered. Show all changes and retry.
failure-viewer-row-too-large = A diff row is too large to display.
failure-project-catalogue-unavailable = The project catalogue is unavailable. Check the GTL server and try again.
failure-project-already-exists = A project with this ID, title, or source already exists.
failure-project-catalogue-full = The project catalogue already holds { $projects } projects.
failure-project-scan-failed = Could not scan this folder. Check that it exists and is readable.
failure-project-scan-folder-not-absolute = Enter an absolute folder path or one that starts with ~/.
failure-project-scan-folder-not-directory = The scan path is not a folder.
failure-project-scan-folder-not-utf8 = The scan folder path is not valid UTF-8.
failure-project-home-unavailable = The home folder is unavailable.
failure-project-too-many-repositories = This folder contains more than { $repositories } repositories. Choose a narrower folder.
failure-project-comparison-branch-missing = Local comparison branch '{ $branch }' is missing in { $path }.
failure-project-repository-unborn = The repository at { $path } has no commits to compare.
failure-project-no-common-ancestor = Local comparison branch '{ $branch }' and HEAD have no common ancestor in { $path }.
failure-project-scan-stale = This folder changed since the scan. Scan again.
failure-project-already-managed = This repository is already a managed project.
failure-project-commit-count-unavailable = Git could not count the commits ahead of the comparison branch.
failure-repository-not-a-repository = { $path } is not inside a Git repository.
failure-repository-no-repositories = No Git repositories were found under { $root }.
failure-repository-search-failed = Could not search { $path } for repositories.

## Viewer client errors

client-error-protocol-mismatch = This desktop viewer and gtl-server use different versions. Update and restart both.
client-error-disconnected = The desktop viewer is temporarily unavailable.
client-error-invalid-message = The viewer could not read this response. Refresh and try again.
client-error-stream-closed = The viewer stream closed. Refresh to load it again.
client-error-desktop = The desktop window could not complete this action.

## Notifications

toast-viewport = Notifications
toast-dismiss = Dismiss notification
toast-waiting =
    { $count ->
        [one] { $count } more notification
       *[other] { $count } more notifications
    }

## Settings recovery

settings-recovery-label = Settings recovery
settings-recovery-title = User settings are invalid
settings-recovery-message = Repair the file and retry, or restore defaults. Reset saves the original file as config.yyyymmdd-hhmmss-backup.toml before replacing it. Backup timestamps use UTC.
settings-recovery-loading = Loading settings details...
settings-recovery-resetting = Backing up and resetting...
settings-recovery-reset = Back up and reset settings
settings-recovery-valid = The settings file is valid now. Retry to continue.
settings-recovery-reset-done = Settings reset. Backup: { $path }

## Viewer navigation

viewer-settings = Settings

## Push

push-button = Push
push-check-result = Check result
push-check-result-title = Check push result: { $message }
push-waiting = Waiting to push commits...
push-running = Pushing commits...
push-completed = Push completed.
push-not-started = The push did not start. Review it again.
push-status-unreadable = { $error } The push may still be running. Use the push button to check its result.
push-status-pending = The push result is not available yet. Use the push button to check its result.
push-availability-nothing = No unpushed commits through this SHA
push-dialog-title =
    { $count ->
        [one] Push { $count } commit?
       *[other] Push { $count } commits?
    }
push-dialog-description = Commits through { $commit } will be pushed. Newer commits stay local.
push-details-disclosure = Details & command
push-confirm-shortcut = Confirm push (Enter)
push-detail-destination = Destination
push-detail-remote-url = Remote URL
push-detail-project = Project
push-detail-directory = Directory
push-detail-branch = Branch
push-detail-remote-branch = Remote branch
push-detail-commits = Commits to push
push-detail-remote = Remote
push-detail-selected-sha = Selected SHA
push-detail-command = Command
push-command-help = Explain command
push-command-copy = Copy command
push-command-copied = Command copied
push-command-copy-failed = Could not copy command
push-command-arg-directory = Runs Git in this directory.
push-command-arg-mirror = Limits the push to the selected branch, even if the remote has mirroring configured.
push-command-arg-push = Sends commits to the remote.
push-command-arg-atomic = Requires the remote to update all requested references together.
push-command-arg-porcelain = Gives Git Tools structured results for rejected references.
push-command-arg-tags = Leaves related tags local.
push-command-arg-submodules = Does not push submodules.
push-command-arg-separator = Ends the options; the destination follows.
push-command-arg-remote = Names the remote that receives the commits.
push-command-arg-ref = Sends the selected commit to the named remote branch.

## Project status

projects-issue-request-failed = Git status unavailable
projects-issue-repository-absent = Repository not found
projects-issue-head-unavailable = Branch status unavailable
projects-issue-working-tree-unavailable = Working-tree status unavailable
projects-issue-upstream-missing = No upstream configured
projects-local-modified-untracked = Modified and untracked files
projects-local-modified = Modified files
projects-local-untracked = Untracked files
projects-local-clean = Working tree clean
projects-ahead-upstream =
    { $count ->
        [0] Nothing to push
        [one] { $count } unpushed commit
       *[other] { $count } unpushed commits
    }
projects-ahead-branch =
    { $count ->
        [0] Nothing ahead of { $base }
        [one] { $count } commit ahead of { $base }
       *[other] { $count } commits ahead of { $base }
    }
projects-status-loading = Loading Git status
projects-detached-head = Detached HEAD
projects-review-pending = Changes to review
projects-review-clean = Up to date
projects-review-comparison-unavailable = Comparison unavailable
projects-review-status-unavailable = Status unavailable
projects-review-stale = Git status is stale. Last successful values are shown. Retry Git status.
projects-retry-status-for = Retry Git status for { $project }
projects-retry-status = Retry Git status
projects-comparison-branch = Comparison branch
projects-change-comparison-branch = Change comparison branch

## Projects table

projects-table-caption = Managed projects
projects-table-status = Status
projects-table-project = Project
projects-table-branch = Branch
projects-table-changes = Changes
projects-table-actions = Actions
projects-local-counts =
    { $tracked ->
        [one] { $tracked } tracked file changed
       *[other] { $tracked } tracked files changed
    }; { $untracked ->
        [one] { $untracked } untracked file
       *[other] { $untracked } untracked files
    }
projects-tracked-changes = Tracked changes
projects-no-changes = No changes pending
projects-changes-popover = Project changes

## Projects dashboard

projects-add = Add projects
projects-diff-all = Diff unpushed projects
projects-diff-all-empty = No projects have unpushed commits.
projects-diff-all-opened =
    { $count ->
        [one] Opened a diff snapshot for { $count } project.
       *[other] Opened diff snapshots for { $count } projects.
    }
projects-diff-all-warning =
    { $count ->
        [one] { $count } project could not be compared: { $projects }.
       *[other] { $count } projects could not be compared: { $projects }.
    }
projects-snapshots-title = Snapshots
projects-unavailable = Projects unavailable
projects-unavailable-message = Check the project catalogue and try again.
projects-loading = Loading projects
projects-empty = No managed projects
projects-empty-message = Projects managed in Git Tools appear here.
projects-empty-active = No active projects
projects-empty-active-message = Add projects, or choose Paused to find paused ones.
projects-empty-paused = No paused projects
projects-empty-paused-message = Paused projects appear here.
projects-status-filter-label = Filter projects by status
projects-status-active = Active
projects-status-paused = Paused
projects-status-all = All
projects-count =
    { $count ->
        [one] { $count } project
       *[other] { $count } projects
    }
projects-paused-status = Paused
projects-pause = Pause { $project }
projects-resume = Resume { $project }
projects-paused = Paused { $project }.
projects-resumed = Resumed { $project }.
projects-edit = Edit { $project }
projects-edit-short = Edit project
projects-edit-title = Edit { $project }
projects-edit-save = Save
projects-edit-discard-title = Discard unsaved changes?
projects-edit-discard-description = Your changes to { $project } have not been saved.
projects-edit-discard = Discard changes
projects-snapshot-history = Snapshot history
projects-per-page = Per page
projects-per-page-label = Projects per page
projects-all-snapshots = All snapshots
projects-comparison-branch-value = Comparison branch: { $branch }
projects-comparison-branch-hint = Used when the current branch has no upstream.
projects-comparison-save = Save comparison
projects-import-folder = Folder to scan
projects-import-folder-placeholder = ~/my-projects or /path/to/projects
projects-import-choose-folder = Choose folder
projects-import-scan = Scan
projects-import-scanning = Scanning folders…
projects-import-found =
    { $found ->
        [one] { $found } repository found
       *[other] { $found } repositories found
    } · { $selected } selected
projects-import-deselect-all = Deselect all
projects-import-select-all = Select all
projects-import-none-found = No Git repositories found in this folder.
projects-import-empty = Choose a folder to find Git repositories. Results start unchecked.
projects-import-footer = Only selected repositories are added. Each result is reported separately.
projects-import-add = Add { $selected } selected
projects-import-created = Created
projects-import-restored = Restored
projects-import-failed = Failed
projects-import-new = New
projects-import-active = Active project
projects-import-paused = Paused project
projects-import-unmanaged = Unmanaged · Restore
projects-import-select-row = Select { $path }
projects-import-id-label = Project ID for { $project }
projects-import-title-label = Project title for { $project }

## Recipe labels

recipe-label-unpushed = { $repository }: diff
recipe-label-unpushed-commits =
    { $count ->
        [one] { $repository }: { $count } commit
       *[other] { $repository }: { $count } commits
    }
recipe-label-working-tree = { $repository }: { $base }->working
recipe-label-commit = { $repository }: { $rev }^!
recipe-label-compared = { $repository } | { $base }->{ $head }
recipe-label-working-tree-head = working
recipe-label-range = { $repository }: { $range }
recipe-label-merge-into = { $repository }: merge ->{ $base }
recipe-label-merge = { $repository }: merge { $branch }->{ $upstream }
recipe-label-last-commits =
    { $count ->
        [one] { $repository }: last { $count } commit
       *[other] { $repository }: last { $count } commits
    }

## Tabs

tab-snapshot-name = Snapshot name
tab-sortable = sortable tab
tab-unpin-named = Unpin { $tab }
tab-unpin = Unpin tab
tab-pin = Pin tab
tab-close-named = Close { $tab }
tab-close = Close tab
tab-close-others = Close others
tab-rename-snapshot = Rename snapshot
tab-actions = Tab actions
tab-live = Live
tab-state-ready = Ready
tab-state-rendering = Rendering
tab-state-stopped = Render stopped
tab-state-failed = Render failed
tabs-open-count =
    { $count ->
        [one] { $count } open diff
       *[other] { $count } open diffs
    }

## Shared controls

dialog-close = Close dialog
dialog-cancel = Cancel
dialog-close-named = Close { $title }
dialog-close-short = Close
no-data = No data
pagination-position = { $label } page position
pagination-pages = { $label } pages
pagination-first = First page
pagination-previous = Previous page
pagination-next = Next page
pagination-last = Last page
pagination-page-of = Page { $number } of { $count }
scrollbar-diff-horizontal = Scroll diff horizontally
scrollbar-horizontal = Scroll horizontally
scrollbar-vertical = Scroll vertically
table-sort-descending = Sort { $column } descending
table-sort-ascending = Sort { $column } ascending
table-sort-by = Sort by { $column }
inline-editor-hint = Enter to save, Escape to cancel
inline-editor-saving = Saving name
extensions-none = No extensions selected
extensions-add-extension = Add extension…
extensions-remove = Remove .{ $extension }
extensions-search = Search or add an extension
extensions-search-placeholder = Search or add…
extensions-clear-search = Clear extension search
extensions-options = Extensions to filter
extensions-in-diff = In this diff
extensions-other = Other extensions
extensions-more = Search to find more extensions
extensions-toggle-named = Toggle .{ $extension }
extensions-add = Add .{ $extension }
extensions-invalid = Enter a final file extension, such as .lock or .md.

## Diff document

diff-view-title-diff = diff
diff-view-title-merge-diff = merge-diff
diff-view-title-commit = commit { $commit }
file-status-added = Added file
file-status-deleted = Deleted file
file-status-renamed = Renamed file
file-status-modified = Modified file
diff-line-omitted =
    { $count ->
        [one] ... (+{ $count } character omitted)
       *[other] ... (+{ $count } characters omitted)
    }
diff-search-all-files-label = Find code in all files
diff-search-all-files-placeholder = Search code in all files...
diff-search-code = Search code
diff-search-previous = Previous match
diff-search-previous-title = Previous match (Shift+Enter)
diff-search-next = Next match
diff-search-next-title = Next match (Enter)
diff-search-close = Close search
diff-search-close-title = Close search (Escape)
diff-find-query-too-long = Search is limited to { $bytes } UTF-8 bytes.
diff-find-searching = Searching…
diff-find-no-matches = No matches
diff-find-matches =
    { $count ->
        [one] { $count } match
       *[other] { $count } matches
    }
diff-find-matches-wrapped =
    { $count ->
        [one] { $count } match · wrapped
       *[other] { $count } matches · wrapped
    }
diff-rendered = Rendered diff
diff-rendered-for = Rendered diff for { $title }
diff-rows-label =
    { $layout ->
        [split] split
       *[unified] unified
    } { $density ->
        [full] full
       *[compact] compact
    } diff rows
diff-preparing = Preparing diff…
diff-source-unavailable = Diff source is unavailable
diff-source-unavailable-message = Refresh this tab to try loading its diff again.
diff-refresh = Refresh
diff-open-in-editor = Open in text editor
copy-file-path = Copy file path
copy-copied = Copied
copy-failed = Failed
copy-relative-path = Relative path
copy-absolute-path = Absolute path
copy-relative-path-action = Copy relative path
copy-absolute-path-action = Copy absolute path
copy-context = Copied with context
copy-context-lines = Copied with context - lines { $lines }
copy-context-files =
    { $count ->
        [one] Copied with context - { $count } file
       *[other] Copied with context - { $count } files
    }
copy-selection-failed = Could not copy the selected text.
copy-selection-empty = The selection contains no source lines.
copy-selection-pending = Copying selected source…

## Diff rows

diff-rows-disconnected = The diff row stream disconnected.
diff-rows-invalid = The server returned invalid diff rows. Retry this view to load it again.

## Snapshot history

history-kind-diff = Diff
history-kind-merge-diff = Merge diff
history-filter-all = All projects
history-filter-unassociated = Unassociated
history-project = Project
history-project-label = Snapshot project
history-renders = Recent diff renders
history-unavailable = Snapshots are unavailable
history-empty = No snapshots yet
history-empty-message = Create a snapshot to save this project's comparison.
history-column-id = ID
history-column-diff = Diff
history-column-kind = Kind
history-column-range = Range
history-column-rendered = Rendered
history-loading = Loading history
history-open = Open
history-open-named = Open { $title }
history-copy-json = Copy render JSON
history-copy-json-named = Copy { $title } JSON
history-label = History
history-render-count =
    { $count ->
        [one] { $count } render
       *[other] { $count } renders
    }

## Diff workspace

workspace-heading = Diff viewer
workspace-panels = Viewer panels
workspace-files = Files
workspace-changed-files = Changed files
workspace-commits = Commits
workspace-unavailable = Viewer state is unavailable
workspace-loading = Loading viewer
workspace-active-diff = Active diff
workspace-empty = No diff is open
workspace-empty-message = Run a git-tools diff command or open a project to see a diff.
workspace-source-unavailable = Repository unavailable
workspace-no-changes = No changes
workspace-no-changes-message = No changes in this comparison. Refresh the tab to check again.
workspace-no-changes-live-message = No changes in this comparison. Updates appear automatically when HEAD changes.
workspace-live-warnings = Live diff update warnings
workspace-live = Live
workspace-live-start-hint = Update this tab whenever the repository changes
workspace-live-stop-hint = Stop updating this tab and keep its current snapshot
workspace-refresh = Refresh
workspace-refresh-hint = Show the latest changes for this tab's range
workspace-live-recent-errors = Recent update errors
workspace-live-occurrences =
    { $count ->
        [one] Occurred { $count } time
       *[other] Occurred { $count } times
    }
workspace-modified-files = Modified files
workspace-modified-files-hint = Inspect current staged, unstaged, and untracked changes against HEAD
commits-empty = No commits
commits-loading = Loading commits...
commits-load-more = Load more
commits-select = Select commit { $commit }: { $subject }
commits-merge = merge
commits-details-for = Commit details for { $commit }
commits-details = Commit details
commits-date = Date
commits-id = Commit ID
commits-copy-id = Copy commit ID

## Diff workspace chrome

files-empty = No changed files
files-count =
    { $count ->
        [one] { $count } file
       *[other] { $count } files
    }
files-sort = Sort files
files-sort-selected =
    { $sort ->
        [changes] Sort files: most changed
       *[path] Sort files: path
    }
files-sort-path = Path
files-sort-changes = Most changed
sidebar-visibility = Sidebar visibility
sidebar-toggle-files = Toggle Files sidebar
sidebar-toggle-commits = Toggle Commits sidebar

## Path filter

path-filter-label = Find a file by path
path-filter-results = Matching files
path-filter-empty = No files match
path-filter-searching = Searching files...

## Diff file filters

file-filters-label = Filter files
file-filters-label-hidden =
    { $count ->
        [one] Filter files: { $count } file hidden
       *[other] Filter files: { $count } files hidden
    }
file-filters-text = Text in files
file-filters-text-placeholder = Search code…
file-filters-changes = Changes
file-filters-change-added = Added
file-filters-change-removed = Removed
file-filters-change-modified = Modified
file-filters-change-modified-hint = Includes renamed files
file-filters-since = Changed since
file-filters-since-notice = Since
file-filters-since-any-time = Any time
file-filters-since-last-hour = Last hour
file-filters-since-today = Today
file-filters-since-last-24-hours = Last 24 hours
file-filters-since-last-7-days = Last 7 days
file-filters-since-custom = Custom…
file-filters-since-custom-label = Changed since date and time
file-filters-since-description = Shows only changes committed after this time. Uncommitted changes stay visible.
file-filters-since-unavailable = Modified files show only uncommitted changes.
file-filters-extensions = Extensions
file-filters-hidden-count =
    { $count ->
        [one] { $count } file hidden
       *[other] { $count } files hidden
    }
file-filters-clear = Clear filters
file-filters-no-matches = No files match the filters
file-filters-no-matches-message = Change or clear the filters to show the hidden files.
extensions-selected = Filtered extensions
extensions-mode-label = Filter mode
extensions-mode-only = Show only
extensions-mode-hide = Hide
extensions-mode-only-description = Show only changed files with these extensions.
extensions-mode-hide-description = Hide changed files with these extensions.
extensions-too-many-changes = Too many pending filter changes. Try again shortly.

## Project comparisons

project-diff-failed = Could not open comparison
project-diff-opening = Opening mode
project-diff-loading = Loading diff...

## Offline artifacts


## Desktop host

tray-show = Show
tray-quit = Quit
projects-import-picker-title = Choose a folder to scan

files-expand-diffs = Expand all diffs
files-collapse-diffs = Collapse all diffs
tab-details = Comparison details
tab-details-base = Base
tab-details-head = Head
tab-push-activate = Open this tab to review a push.
settings-back = Back
settings-appearance = Appearance
settings-locale = Language & dates
settings-snapshots = Diff tab
settings-git = Git
settings-autosave = Changes save automatically

projects-viewer-push-no-confirmation = No confirmation on push in the viewer
projects-viewer-push-no-confirmation-hint = Push runs immediately when enabled. Off by default to prevent accidental pushes.
review-actions-label = Diff review actions
review-push = Push
review-push-check = Check
review-close-label = Close
review-unpushed-hint = This snapshot contains commits that have not been pushed.
review-close = Close diff
review-close-pinned = Unpin this diff before closing it.
review-previous-diff = Previous diff
review-next-diff = Next diff
review-actions-hide = Hide review actions
review-actions-show = Show review actions

commit-search-label = Find commits
commit-search-placeholder = Search…
commit-search-hint = Message or hash
commit-search-time-from = From
commit-search-time-until = Until
commit-search-time-hint = Local time; either bound can be empty.
commit-search-time-clear = Clear time filter
commit-search-time-invalid = Enter a valid local date and time.
commit-search-time-reversed = Until must be at or after From.
commit-search-snapshot = Snapshot
commit-search-branch = Branch
commit-search-scope = Search scope
commit-search-snapshot-option = This snapshot
commit-search-branch-option = Active branch
commit-search-close = Close commit search
commit-search-clear = Clear commit search
commit-search-snapshot-hint = Only commits in this snapshot.
commit-search-branch-hint = Local commits on the active branch.
commit-search-project-title = Find commits · { $project }
commit-search-loading = Searching commits…
commit-search-count =
    { $total ->
        [0] No matching commits
        [one] 1 match
       *[other] { $total } matches
    }
commit-search-count-limited = { $shown } of { $total } matches
commit-search-results = Matching commits
commit-search-open = Open commit { $id }: { $subject }
commit-search-empty = Try a shorter query, a commit hash, or a wider time range.
commit-search-detached = Check out a branch to search its commits.
