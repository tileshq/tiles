---------------------------- MODULE Painter ----------------------------
(***************************************************************************)
(* The wrapper painter's kitty motion route (crates/tiles-cli/src/wrap/    *)
(* paint.rs, Route::KittyPlaceholders with Painter::with_motion): what     *)
(* the painter remembers about each card, and what kitty holds as a        *)
(* result of the commands it sends.                                        *)
(*                                                                         *)
(* One action is one call the wrapper makes into the painter (feed, tick,  *)
(* resize) or one thing the harness or the clock does. A painter call      *)
(* runs to its end before anything else happens, as in the code: the      *)
(* painter is single-threaded and owns the terminal while it writes.       *)
(* The harness types each block a row at a time and may hold a            *)
(* synchronized update (CSI ? 2026 h) around any of it.                   *)
(*                                                                         *)
(* Names follow the code: uploaded, sending, entrances (entr), shown,     *)
(* whole, early, broken. Kitty's images and placements are pairs          *)
(* <<card, kind>> with kind "card" (card_image_id), "entr" (the           *)
(* entrance's image from take_image_id) or "mask" (the backing).          *)
(***************************************************************************)
EXTENDS Naturals, FiniteSets

CONSTANTS Cards,    \* card ids, such as {"a1b2c3", "00aa11"}
          Rows      \* rows in each card's block

Kinds == {"card", "entr", "mask"}
Plays == {"none", "Ready", "Waiting", "Playing", "Done"}  \* enum Play, "none" = not in entrances

VARIABLES
    typed,     \* rows of the card's block the harness has typed, from row 1
    hsync,     \* Tracker::harness_sync: the harness holds a synchronized update
    cached,    \* the card is in the cache (Pictures::fresh listed it)
    uploaded,  \* Painter::uploaded
    sending,   \* Painter::sending: entrance frames still in the outbox
    entr,      \* Painter::entrances[c].state
    clock,     \* "over" once SYNC_WAIT (Waiting) or Entrance::length (Playing) has passed
    shown,     \* Painter::shown
    whole,     \* Painter::whole: never cleared
    early,     \* Painter::early[c].image: "none", "card" or "entr"
    broken,    \* Painter::broken
    imgs,      \* images kitty holds
    places     \* placements kitty shows: the early placement (p=2) and the backing (p=1)

vars == <<typed, hsync, cached, uploaded, sending, entr, clock, shown, whole,
          early, broken, imgs, places>>

TypeOK ==
    /\ typed \in [Cards -> 0..Rows]
    /\ hsync \in BOOLEAN /\ broken \in BOOLEAN
    /\ cached \in [Cards -> BOOLEAN] /\ uploaded \in [Cards -> BOOLEAN]
    /\ sending \in [Cards -> BOOLEAN] /\ shown \in [Cards -> BOOLEAN]
    /\ whole \in [Cards -> BOOLEAN]
    /\ entr \in [Cards -> Plays]
    /\ clock \in [Cards -> {"running", "over"}]
    /\ early \in [Cards -> {"none", "card", "entr"}]
    /\ imgs \subseteq (Cards \X Kinds) /\ places \subseteq (Cards \X Kinds)

Init ==
    /\ typed = [c \in Cards |-> 0]
    /\ hsync = FALSE /\ broken = FALSE
    /\ cached = [c \in Cards |-> FALSE] /\ uploaded = [c \in Cards |-> FALSE]
    /\ sending = [c \in Cards |-> FALSE] /\ shown = [c \in Cards |-> FALSE]
    /\ whole = [c \in Cards |-> FALSE]
    /\ entr = [c \in Cards |-> "none"]
    /\ clock = [c \in Cards |-> "running"]
    /\ early = [c \in Cards |-> "none"]
    /\ imgs = {} /\ places = {}

(***************************************************************************)
(* Kitty's side (kitty.rs): a = d, d = I deletes an image and every       *)
(* placement of it; a = d, d = i deletes one placement; a = p places.      *)
(***************************************************************************)
Delete(I, x) == I \ {x}
Unplace(P, x) == P \ {x}

(***************************************************************************)
(* The painter's steps, as operators over a record of the painter state,  *)
(* so one call can chain them in the code's order.                        *)
(***************************************************************************)
State == [uploaded |-> uploaded, sending |-> sending, entr |-> entr,
          clock |-> clock, shown |-> shown, whole |-> whole, early |-> early,
          imgs |-> imgs, places |-> places]

Done(s) ==
    /\ uploaded' = s.uploaded /\ sending' = s.sending /\ entr' = s.entr
    /\ clock' = s.clock /\ shown' = s.shown /\ whole' = s.whole
    /\ early' = s.early /\ imgs' = s.imgs /\ places' = s.places

\* Painter::upload: the settled card once, and its entrance queued unless
\* the card has shown settled already.
Upload(s, c) ==
    IF s.uploaded[c] THEN s
    ELSE [s EXCEPT !.uploaded[c] = TRUE,
                   !.imgs = @ \cup {<<c, "card">>},
                   !.sending[c] = ~s.shown[c]]

\* Painter::settle: the card shows settled; its entrance, sent or not, is
\* dropped (outbox frames, `sending`, or an entrance still Ready).
Settle(s, c) ==
    IF s.shown[c] THEN s
    ELSE IF s.sending[c] \/ s.entr[c] = "Ready"
         THEN [s EXCEPT !.shown[c] = TRUE, !.sending[c] = FALSE,
                        !.entr[c] = "none",
                        !.imgs = Delete(@, <<c, "entr">>),
                        !.places = Unplace(@, <<c, "entr">>)]
         ELSE [s EXCEPT !.shown[c] = TRUE]

\* Painter::start: show the first visible entrance frame; its clock runs now,
\* or once the harness's update has ended.
Start(s, c) ==
    IF s.entr[c] # "Ready" THEN s
    ELSE [s EXCEPT !.entr[c] = IF hsync THEN "Waiting" ELSE "Playing",
                   !.clock[c] = "running"]

\* Painter::unplace_early.
UnplaceEarly(s, c) ==
    IF s.early[c] = "none" THEN s
    ELSE LET img == <<c, s.early[c]>>
             p1 == Unplace(Unplace(s.places, img), <<c, "mask">>)
             i1 == Delete(s.imgs, <<c, "mask">>)
         IN IF s.early[c] = "entr"
            THEN [s EXCEPT !.early[c] = "none", !.entr[c] = "none",
                           !.places = Unplace(p1, <<c, "entr">>),
                           !.imgs = Delete(i1, <<c, "entr">>)]
            ELSE [s EXCEPT !.early[c] = "none", !.places = p1, !.imgs = i1]

\* Painter::place_early: the entrance's image when it is in the terminal,
\* else the settled card (which then stays settled); the backing under it.
PlaceEarly(s0, c) ==
    LET s == Upload(s0, c)
        img == IF s.early[c] = "entr" /\ s.entr[c] # "none" THEN "entr"
               ELSE IF s.early[c] = "none" /\ s.entr[c] = "Ready" THEN "entr"
               ELSE "card"
        s1 == IF img = "card" THEN Settle(s, c) ELSE Start(s, c)
    IN [s1 EXCEPT !.early[c] = img,
                  !.imgs = @ \cup {<<c, "mask">>},
                  !.places = @ \cup {<<c, img>>, <<c, "mask">>}]

Playing(s, c) == s.entr[c] \in {"Waiting", "Playing"}

\* Painter::paint for card c, the block typed down to row typed[c]:
\* Painter::draw_placeholders once the block is whole, then early_copies and
\* place_all_early.
Paint(s0, c) ==
    LET n == typed[c]
        s == IF n = Rows THEN [s0 EXCEPT !.whole[c] = TRUE] ELSE s0
        best == s.whole[c] /\ n = Rows
        ready == s.entr[c] = "Ready"
        s1 == IF best
              THEN LET u == Upload(s, c)
                   IN IF u.early[c] = "none" /\ ~(u.entr[c] = "Ready")
                      THEN Settle(u, c) ELSE u
              ELSE s
        \* early_copies: a block still being typed, or a whole block whose
        \* entrance is Ready (typed in one go) or playing while placed early.
        wanted == \/ (~s1.whole[c] /\ n >= 1)
                  \/ (best /\ s1.early[c] = "none" /\ s1.entr[c] = "Ready")
                  \/ (best /\ s1.early[c] # "none" /\ Playing(s1, c))
    IN IF wanted THEN PlaceEarly(s1, c) ELSE UnplaceEarly(s1, c)

\* Painter::step: run, stop, and hand over each early card whose block is
\* whole and whose entrance has played.
Step(s0) ==
    LET s == [s0 EXCEPT !.entr =
                 [c \in Cards |->
                    CASE s0.entr[c] = "Waiting" /\ (~hsync \/ s0.clock[c] = "over")
                           -> "Playing"
                      [] s0.entr[c] = "Playing" /\ s0.clock[c] = "over" -> "Done"
                      [] OTHER -> s0.entr[c]],
               !.clock = [c \in Cards |->
                    IF s0.entr[c] = "Waiting" /\ (~hsync \/ s0.clock[c] = "over")
                    THEN "running" ELSE s0.clock[c]]]
        over == {c \in Cards : s.early[c] # "none" /\ s.whole[c] /\ ~Playing(s, c)}
        Hand[S \in SUBSET Cards] ==
            IF S = {} THEN s
            ELSE LET c == CHOOSE x \in S : TRUE IN UnplaceEarly(Hand[S \ {c}], c)
    IN Hand[over]

\* Painter::flush: the entrance's frames go out over several ticks; the
\* first command (a = t) makes the image, the last makes it Ready.
SendSome(c) ==
    /\ ~broken /\ sending[c]
    /\ imgs' = imgs \cup {<<c, "entr">>}
    /\ UNCHANGED <<typed, hsync, cached, uploaded, sending, entr, clock, shown,
                   whole, early, broken, places>>

SendRest(c) ==
    /\ ~broken /\ sending[c]
    /\ sending' = [sending EXCEPT ![c] = FALSE]
    /\ entr' = [entr EXCEPT ![c] = "Ready"]
    /\ imgs' = imgs \cup {<<c, "entr">>}
    /\ UNCHANGED <<typed, hsync, cached, uploaded, clock, shown, whole, early,
                   broken, places>>

(***************************************************************************)
(* The calls.                                                              *)
(***************************************************************************)
\* Painter::tick: upload the cards fresh in the cache, then step.
Tick ==
    /\ ~broken
    /\ LET Up[S \in SUBSET Cards] ==
               IF S = {} THEN State
               ELSE LET c == CHOOSE x \in S : TRUE IN Upload(Up[S \ {c}], c)
       IN Done(Step(Up[{c \in Cards : cached[c]}]))
    /\ UNCHANGED <<typed, hsync, cached, broken>>

\* Painter::feed of a chunk that ends at ground: the paint for every card
\* on screen (paint), then step (paint_now). Cards are independent here, so
\* one card's paint per call covers every order the code takes them in.
PaintNow(c) ==
    /\ ~broken /\ cached[c]
    /\ Done(Step(Paint(State, c)))
    /\ UNCHANGED <<typed, hsync, cached, broken>>

\* Painter::resize: forget_placements removes every early placement.
Resize ==
    /\ ~broken
    /\ LET F[S \in SUBSET Cards] ==
               IF S = {} THEN State
               ELSE LET c == CHOOSE x \in S : TRUE IN UnplaceEarly(F[S \ {c}], c)
       IN Done(F[Cards])
    /\ UNCHANGED <<typed, hsync, cached, broken>>

\* Painter::give_up after the screen model panicked: unplace_early for each
\* card placed early, then the painter draws nothing more. (Before the fix
\* it sent delete_placement(card_image_id(id), EARLY_PLACEMENT) and
\* delete(early.mask), which left an entrance placed early on screen.)
GiveUp ==
    /\ ~broken
    /\ broken' = TRUE
    /\ LET F[S \in SUBSET Cards] ==
               IF S = {} THEN State
               ELSE LET c == CHOOSE x \in S : TRUE IN UnplaceEarly(F[S \ {c}], c)
       IN Done(F[Cards])
    /\ UNCHANGED <<typed, hsync, cached>>

\* The harness and the clock.
Type(c) ==
    /\ typed[c] < Rows
    /\ typed' = [typed EXCEPT ![c] = @ + 1]
    /\ UNCHANGED <<hsync, cached, uploaded, sending, entr, clock, shown, whole,
                   early, broken, imgs, places>>

Arrive(c) ==
    /\ ~cached[c]
    /\ cached' = [cached EXCEPT ![c] = TRUE]
    /\ UNCHANGED <<typed, hsync, uploaded, sending, entr, clock, shown, whole,
                   early, broken, imgs, places>>

SyncBegin == /\ ~hsync /\ hsync' = TRUE
             /\ UNCHANGED <<typed, cached, uploaded, sending, entr, clock, shown,
                            whole, early, broken, imgs, places>>
SyncEnd == /\ hsync /\ hsync' = FALSE
           /\ UNCHANGED <<typed, cached, uploaded, sending, entr, clock, shown,
                          whole, early, broken, imgs, places>>

Elapse(c) ==
    /\ entr[c] \in {"Waiting", "Playing"} /\ clock[c] = "running"
    /\ clock' = [clock EXCEPT ![c] = "over"]
    /\ UNCHANGED <<typed, hsync, cached, uploaded, sending, entr, shown, whole,
                   early, broken, imgs, places>>

Next ==
    \/ Tick \/ Resize \/ GiveUp \/ SyncBegin \/ SyncEnd
    \/ \E c \in Cards : PaintNow(c) \/ SendSome(c) \/ SendRest(c)
                        \/ Type(c) \/ Arrive(c) \/ Elapse(c)

Spec == Init /\ [][Next]_vars

\* The painter keeps being called (the wrapper ticks every few ms and feeds
\* every read), the clock moves and the harness finishes typing. One weak
\* fairness over all of it: one per action makes TLC check every subset.
Progress ==
    \/ Tick
    \/ \E c \in Cards : PaintNow(c) \/ SendRest(c) \/ Elapse(c) \/ Type(c)

FairSpec == Spec /\ WF_vars(Progress)

(***************************************************************************)
(* The properties.                                                         *)
(***************************************************************************)

\* Kitty is never asked to place a picture it does not hold.
PlacedPicturesExist == places \subseteq imgs

\* A painter that gave up leaves no early placement on screen: a stuck
\* picture would cover the harness's text with nothing to remove it.
GaveUpCleanly == broken => places = {}

\* Once a card's block is whole and its early placement handed over, kitty
\* keeps one picture for it: no entrance image and no backing.
OnePicturePerCard ==
    \A c \in Cards :
        (~broken /\ whole[c] /\ early[c] = "none")
            => (<<c, "entr">> \notin imgs /\ <<c, "mask">> \notin imgs)

\* An entrance starts running only when the harness holds no synchronized
\* update, or SYNC_WAIT after it was cued (kitty shows nothing new inside
\* an update while the animation's clock runs).
RunsOutsideSync ==
    [][\A c \in Cards :
         (entr'[c] = "Playing" /\ entr[c] # "Playing")
            => (~hsync \/ clock[c] = "over")]_vars

\* An entrance placed early ends: the card is handed over to its block.
EntranceEnds == \A c \in Cards : early[c] = "entr" ~> early[c] # "entr"

=============================================================================
