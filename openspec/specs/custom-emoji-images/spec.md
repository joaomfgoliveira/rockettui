# custom-emoji-images Specification

## Purpose

Draws the server's custom emojis as images in messages and reactions on terminals that can show them, and as a styled shortcode everywhere else, loading and caching the images safely as a normal client.

## Requirements

### Requirement: The custom emoji list is loaded at session start
The client SHALL request the server's custom emoji list once when the session starts and keep it for the session. A failed request SHALL NOT stop the session or the message list: custom shortcodes are drawn as the fallback until a later request succeeds, and the client SHALL try again the next time a room is opened.

#### Scenario: Session start
- **WHEN** the rooms of a session have loaded
- **THEN** the custom emoji list is requested once, without blocking the message list

#### Scenario: List fails
- **WHEN** the request fails
- **THEN** the session goes on, custom shortcodes are drawn as the fallback, and opening another room requests the list again

#### Scenario: List already loaded
- **WHEN** a room is opened after a successful load
- **THEN** no new request is made

### Requirement: Images are fetched as a normal client
The client SHALL fetch a custom emoji's image only when a message or reaction that is drawn names it, from the server's own emoji path, with the user's own credentials, and at most once per name and `etag` per session. At most a small fixed number of fetches SHALL run at the same time, and the rest SHALL wait. No request SHALL carry message text, room names or usernames.

#### Scenario: Used emoji
- **WHEN** a room is opened whose messages use `:rtshipit:` five times
- **THEN** its image is requested once

#### Scenario: Unused emoji
- **WHEN** the list holds an emoji no drawn message or reaction names
- **THEN** its image is not requested

#### Scenario: Shown only in a list
- **WHEN** a search result, its context view or a row of the reaction picker names a custom emoji
- **THEN** its image is requested like for a message in the open room

#### Scenario: Many at once
- **WHEN** a room needs more images than the limit allows at once
- **THEN** the excess wait for a free slot and none is dropped

### Requirement: Images are cached
A fetched image SHALL be kept for the session and on disk under the client's cache directory, keyed by the emoji's name and the `etag` the server lists. An image whose `etag` changed SHALL be fetched again, and one whose `etag` is unchanged SHALL be read from disk on a later session without a request.

#### Scenario: Next session
- **WHEN** the client starts again and the list shows the same `etag`
- **THEN** the image is read from disk and not requested

#### Scenario: Changed image
- **WHEN** the list shows a different `etag` for a name
- **THEN** the cached image is not used and the new one is fetched

#### Scenario: Cache cannot be written
- **WHEN** the cache directory cannot be created or written
- **THEN** images are kept in memory for the session and no error stops the client

### Requirement: Untrusted images are limited
Image bytes from the server SHALL be treated as untrusted. A response over a fixed size, an image over fixed pixel dimensions, or bytes that do not decode SHALL be refused, and the emoji SHALL be drawn as the fallback. A decoding failure SHALL NOT crash the client and SHALL NOT show the server's text.

#### Scenario: Oversized response
- **WHEN** an emoji's image is larger than the size limit
- **THEN** it is not decoded and the emoji is drawn as the fallback

#### Scenario: Not an image
- **WHEN** the response is not a decodable image
- **THEN** the emoji is drawn as the fallback and the client goes on

#### Scenario: Animated image
- **WHEN** the image is an animated gif or webp
- **THEN** its frames (at most 64) are each sent to the terminal as an image of their own and the client shows them in turn in a loop, redrawing only while an animated emoji is on screen; a frame delay shorter than 20 ms is raised to 20 ms

### Requirement: Images are drawn only where they can be shown
The client SHALL draw an image only when the terminal answered the graphics query at startup as supporting the kitty graphics protocol with Unicode placeholders. In every other case, including a query that gets no answer in a short fixed time, custom emojis SHALL be drawn as the fallback and the client SHALL NOT send any graphics data.

#### Scenario: Supporting terminal
- **WHEN** the terminal supports the protocol
- **THEN** a custom emoji whose image is ready is drawn as its image

#### Scenario: Other terminal
- **WHEN** the terminal does not support it
- **THEN** every custom emoji is drawn as the fallback and no graphics data is sent

#### Scenario: No answer
- **WHEN** the terminal does not answer the query in time
- **THEN** the client starts normally and draws the fallback

### Requirement: The fallback is the shortcode in a theme style
A custom emoji that cannot be drawn as an image SHALL be drawn as its full `:name:` in the theme's custom-emoji style when the terminal cannot show images or the image failed. While an image that can still arrive is not ready, the emoji SHALL be drawn in the same two columns the image will take, so that a row never changes its width or its wrapping when the image becomes ready.

#### Scenario: Terminal without images
- **WHEN** the terminal cannot show images and a message names `:rtshipit:`
- **THEN** the row shows `:rtshipit:` in the custom-emoji style, and nothing about the row changes later

#### Scenario: Not ready yet
- **WHEN** the terminal can show images and the image has not arrived
- **THEN** the emoji takes two columns in the custom-emoji style, and when the image arrives the same cells show it and no row moves

#### Scenario: Image failed
- **WHEN** the fetch or decoding failed
- **THEN** the full `:name:` is drawn in the custom-emoji style and the row is re-measured once, as a failure is final for the session

#### Scenario: Theme style
- **WHEN** the theme sets a style for custom emojis
- **THEN** the fallback is drawn in that style

### Requirement: Server text cannot draw an image
Text from the server SHALL NOT be able to produce an image or any graphics control data. Characters the client uses to place an image SHALL be removed from server text before it is drawn, and an image SHALL appear only where the client itself resolved a custom emoji.

#### Scenario: Placement characters in a message
- **WHEN** a message body contains the placement characters of the graphics protocol
- **THEN** they are removed and nothing is drawn as an image

#### Scenario: Placement characters in a name
- **WHEN** a username, room name or reaction name contains them
- **THEN** they are removed there too

#### Scenario: Escape sequences
- **WHEN** a message contains a terminal escape sequence for graphics
- **THEN** it is drawn as inert text, as every other control sequence already is
