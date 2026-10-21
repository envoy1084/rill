---------------------------- MODULE Sequencer ----------------------------
(***************************************************************************)
(* The landing sequencer, its lease and group commit.                      *)
(*                                                                         *)
(* Landers race for a lease whose epoch fences trunk writes. A lander      *)
(* snapshots the trunk head, the lease epoch and the approvals of a batch  *)
(* of queued requests (speculation), then commits the batch in one         *)
(* transaction that checks the head, the epoch and every approval it saw.  *)
(* Reviews can withdraw approval at any moment, and a lease can lapse      *)
(* while its holder is mid-batch. Rebases are abstracted: the speculative  *)
(* outcome of a batch is any subsequence of the queued requests, in queue  *)
(* order (requests dropped by conflicts or failed checks are left out).    *)
(***************************************************************************)
EXTENDS Naturals, Sequences, FiniteSets

CONSTANTS Landers, Order, MaxEpoch, None

Requests == {Order[i] : i \in 1..Len(Order)}
NoSnap == [head |-> 0, epoch |-> 0, batch |-> <<>>]

VARIABLES
    trunk,      \* the landed requests, in trunk order
    status,     \* request -> "queued" | "landed" | "ejected" | "conflict"
    approved,   \* request -> whether its review still allows landing
    holder,     \* who holds the lease, or None
    epoch,      \* the lease epoch; bumped on every acquisition
    mine,       \* lander -> the epoch it took the lease at (0: never)
    snap,       \* lander -> its in-flight batch (NoSnap: none)
    ghostOk     \* request -> approval held in the transaction that landed it

vars == <<trunk, status, approved, holder, epoch, mine, snap, ghostOk>>

Range(s) == {s[i] : i \in DOMAIN s}

\* Subsequences of `s`, keeping order.
RECURSIVE Subseqs(_)
Subseqs(s) ==
    IF s = <<>> THEN {<<>>}
    ELSE LET rest == Subseqs(Tail(s)) IN
         rest \cup {<<Head(s)>> \o r : r \in rest}

Queued == {r \in Requests : status[r] = "queued"}
QueueSeq == SelectSeq(Order, LAMBDA r : r \in Queued)

Init ==
    /\ trunk = <<>>
    /\ status = [r \in Requests |-> "queued"]
    /\ approved = [r \in Requests |-> TRUE]
    /\ holder = None
    /\ epoch = 0
    /\ mine = [l \in Landers |-> 0]
    /\ snap = [l \in Landers |-> NoSnap]
    /\ ghostOk = [r \in Requests |-> FALSE]

\* Taking a free lease bumps its epoch.
Acquire(l) ==
    /\ holder = None
    /\ epoch < MaxEpoch
    /\ holder' = l
    /\ epoch' = epoch + 1
    /\ mine' = [mine EXCEPT ![l] = epoch + 1]
    /\ UNCHANGED <<trunk, status, approved, snap, ghostOk>>

\* The lease lapses (a pause, a partition); its holder may not know.
Lapse ==
    /\ holder # None
    /\ holder' = None
    /\ UNCHANGED <<trunk, status, approved, epoch, mine, snap, ghostOk>>

\* A review withdraws (or restores) approval at any moment.
Withdraw(r) ==
    /\ status[r] = "queued"
    /\ approved' = [approved EXCEPT ![r] = ~approved[r]]
    /\ UNCHANGED <<trunk, status, holder, epoch, mine, snap, ghostOk>>

\* Speculate: read the head and the approvals, and keep a batch of
\* approved queued requests. Possibly stale by the time it commits.
Speculate(l) ==
    /\ holder = l
    /\ snap[l].batch = <<>>
    /\ \E batch \in Subseqs(QueueSeq) :
        /\ batch # <<>>
        /\ \A i \in DOMAIN batch : approved[batch[i]]
        /\ snap' = [snap EXCEPT ![l] =
                     [head |-> Len(trunk), epoch |-> mine[l], batch |-> batch]]
    /\ UNCHANGED <<trunk, status, approved, holder, epoch, mine, ghostOk>>

\* A request conflicts on rebase and is handed back.
Conflict(l, r) ==
    /\ holder = l
    /\ snap[l].batch = <<>>
    /\ status[r] = "queued"
    /\ mine[l] = epoch
    /\ status' = [status EXCEPT ![r] = "conflict"]
    /\ UNCHANGED <<trunk, approved, holder, epoch, mine, snap, ghostOk>>

\* A request no longer approved is ejected (fenced like every status write).
Eject(l, r) ==
    /\ holder = l
    /\ mine[l] = epoch
    /\ status[r] = "queued"
    /\ ~approved[r]
    /\ status' = [status EXCEPT ![r] = "ejected"]
    /\ UNCHANGED <<trunk, approved, holder, epoch, mine, snap, ghostOk>>

\* Group commit: one transaction checking the head, the lease epoch and
\* every approval it saw, landing the whole batch or nothing.
Commit(l) ==
    /\ snap[l].batch # <<>>
    /\ LET s == snap[l] IN
       IF /\ Len(trunk) = s.head
          /\ epoch = s.epoch
          /\ \A i \in DOMAIN s.batch :
                status[s.batch[i]] = "queued" /\ approved[s.batch[i]]
       THEN /\ trunk' = trunk \o s.batch
            /\ status' = [r \in Requests |->
                            IF r \in Range(s.batch) THEN "landed" ELSE status[r]]
            /\ ghostOk' = [r \in Requests |->
                            IF r \in Range(s.batch) THEN approved[r] ELSE ghostOk[r]]
       ELSE UNCHANGED <<trunk, status, ghostOk>>
    /\ snap' = [snap EXCEPT ![l] = NoSnap]
    /\ UNCHANGED <<approved, holder, epoch, mine>>

Next ==
    \/ \E l \in Landers : Acquire(l) \/ Speculate(l) \/ Commit(l)
    \/ \E l \in Landers, r \in Requests : Conflict(l, r) \/ Eject(l, r)
    \/ \E r \in Requests : Withdraw(r)
    \/ Lapse

Spec == Init /\ [][Next]_vars

-----------------------------------------------------------------------------
\* Safety.

TypeOK ==
    /\ status \in [Requests -> {"queued", "landed", "ejected", "conflict"}]
    /\ epoch \in 0..MaxEpoch

\* No request lands twice.
NoDoubleLand ==
    \A i, j \in DOMAIN trunk : i # j => trunk[i] # trunk[j]

\* Landed exactly when on trunk.
LandedIsOnTrunk ==
    \A r \in Requests : (status[r] = "landed") <=> (r \in Range(trunk))

\* Every landed request was approved in the transaction that landed it.
ApprovedAtCommit ==
    \A r \in Range(trunk) : ghostOk[r]

\* Ejected and conflicted requests never land.
RefusedNeverLands ==
    \A r \in Requests : status[r] \in {"ejected", "conflict"} => r \notin Range(trunk)

Safety ==
    /\ TypeOK /\ NoDoubleLand /\ LandedIsOnTrunk
    /\ ApprovedAtCommit /\ RefusedNeverLands

\* Trunk only grows: nothing landed is ever rewritten or removed.
TrunkAppendOnly ==
    [][\A i \in DOMAIN trunk : i \in DOMAIN trunk' /\ trunk'[i] = trunk[i]]_trunk

\* A lander whose lease was taken over never changes trunk.
Fenced ==
    [][\A l \in Landers :
        (Commit(l) /\ trunk' # trunk) => snap[l].epoch = epoch]_vars
=============================================================================
