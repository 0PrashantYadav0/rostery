# Rostery

Rostery reads one person's listening and watching history on their own machine, roasts it with an AI they already have access to, and draws it as a universe they can publish.

## Language

### History

**Source**:
The service a piece of history came from. Spotify or YouTube.
_Avoid_: Platform, service, provider

**Play**:
One listen to a track or one watch of a video.
_Avoid_: Event, stream, view, history row

**Item**:
The track, podcast episode or video a Play refers to.
_Avoid_: Media, content

**Creator**:
The artist, podcast show or YouTube channel behind an Item.
_Avoid_: Entity, author, channel (when the Source could be either)

**Origin**:
How a Play reached the local store: from an Export, from an API connection, or from demo data.
_Avoid_: Provenance, import type

**Export**:
A file the Source gave the user on request: the Spotify data download or Google Takeout.
_Avoid_: Dump, backup, archive

**Connection**:
An OAuth link between Rostery and one Source, used to fetch recent activity and metadata.
_Avoid_: Integration, login

**Affinity**:
A signal that the user likes a Creator without a Play behind it, such as a Spotify top artist or a YouTube subscription.
_Avoid_: Preference, favourite

**Session**:
A run of Plays with no gap longer than 30 minutes.
_Avoid_: Binge, sitting

**Interest**:
A short lowercase tag describing what a Creator is about, taken from a genre, a topic or an AI Provider.
_Avoid_: Genre (when the tag may not be one), category, label

### Universe

**Universe**:
The map of one person's Creators, for Spotify, YouTube or both together.
_Avoid_: Graph, map, galaxy (when the whole view is meant)

**Planet**:
One Creator in a Universe. Its size follows how long the user listened or how often they watched.
_Avoid_: Node, bubble

**Link**:
A line between two Planets whose Creators share Sessions or Interests.
_Avoid_: Edge, connection, orbit

**Galaxy**:
A group of closely linked Planets.
_Avoid_: Cluster, community, constellation

### Roast

**Digest**:
The aggregated summary of a person's history that a Provider receives. It holds counts and names, never raw Plays.
_Avoid_: Summary, context, payload

**Provider**:
The AI that writes a Roast: the local Claude Code CLI, the local Codex CLI, or a local LLM server.
_Avoid_: Model, backend, agent

**Roast**:
The comedic write-up a Provider produces from a Digest.
_Avoid_: Report, review

**Burn**:
One titled joke inside a Roast.
_Avoid_: Point, section

**Verdict**:
The final judgement closing a Roast: an archetype, a score out of 100 and one sentence.
_Avoid_: Wordict, conclusion, rating

### Sharing

**Share Site**:
The static HTML, CSS and JS folder Rostery generates from what the user chose to share.
_Avoid_: Web app, export (that word belongs to the Source), bundle

**Deploy Target**:
A host Rostery can publish a Share Site to: GitHub Pages or Vercel.
_Avoid_: Destination, host
