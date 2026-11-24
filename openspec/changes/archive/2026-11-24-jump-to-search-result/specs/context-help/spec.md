# Spec Delta

## ADDED Requirements

### Requirement: Popup for search results
While the search results list or a result's context view is open, `?` SHALL open the context popup for it, whichever pane is focused. The popup SHALL name the list ("Search results") or the view ("Search result in context") and list the keys of its actions from the live keymap, plus the fixed `q Esc` row. The titles of the list and of the view SHALL hint at `Enter`, `?` and `q`.

#### Scenario: Popup over the list
- **WHEN** the results list is open and the user presses `?`
- **THEN** the popup names the search results and lists select, the motions and `q Esc`

#### Scenario: Popup over the context view
- **WHEN** a context view is open and the user presses `?`
- **THEN** the popup names the result in context and says select opens it in the chat

#### Scenario: Any key closes it
- **WHEN** the popup is open over the list and the user presses `j`
- **THEN** the popup closes and the selection has not moved

#### Scenario: Title hint
- **WHEN** the list or the view is open
- **THEN** its title mentions `Enter`, `?` and `q`
