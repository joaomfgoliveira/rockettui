# Spec Delta

## ADDED Requirements

### Requirement: Opening a result
While the results list is open, the select action (`Enter` by default) SHALL open the context view of the selected result (see `search-result-context`). The list and its selection SHALL be kept underneath, so returning from the context view shows them as they were.

#### Scenario: Open the selected result
- **WHEN** the list is open with the third result selected and the user presses `Enter`
- **THEN** the context view of that result is requested and, when it arrives, shown

#### Scenario: List is kept
- **WHEN** the user opens a result and then returns from its context view
- **THEN** the list is shown again with the same result selected

#### Scenario: Select with no result
- **WHEN** no list is open and the user presses `Enter` in the messages pane
- **THEN** the behaviour is unchanged by this requirement
