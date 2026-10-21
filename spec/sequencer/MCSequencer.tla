---------------------------- MODULE MCSequencer ----------------------------
\* The model TLC checks: two landers, three requests, three lease epochs.
EXTENDS Sequencer

CONSTANTS r1, r2, r3, l1, l2

MCLanders == {l1, l2}
MCOrder == <<r1, r2, r3>>
=============================================================================
