% Stålmarck's proof procedure: branch execution in Scryer Prolog.
%
% This module runs one Dilemma branch: it asserts the branch assumption on a
% parent relation sent by Rust, then saturates to depth J, including nested
% dilemmas (formal specification §6–§8, ADR-0005). The processing order is
% the canonical Track A order (SPEC-Z3, SPEC-Z4, SPEC-K3), so the counters
% agree with the Rust implementation.
%
% Rust appends the triples to this program as facts (one machine per run):
%   t(Id, Kind, Out, A, B).    Kind is `and` or `eq`; A, B are literal codes
%   occ(Node, TripleIds).      ascending triple ids mentioning Node (> 0)
%   nnodes(N).
%
% A relation is uf(Par, Size, Min, Mem, Contra), a persistent union-find
% with parity over assoc maps. Absent keys mean defaults: a node is its own
% root with parity 0, and a class has size 1, minimum its root and members
% [Root]. Contra is `true` once the relation is contradictory (⊥⊥).

:- module(stal, [branch_entry/6]).

:- use_module(library(assoc)).
:- use_module(library(lists)).
:- use_module(library(iso_ext)).
:- use_module(library(between)).

% --------------------------------------------------------------- counters

counters([proc, att, succ, merges, ins, rem, br, prod]).

reset_counters :-
    counters(Ks),
    maplist(reset_counter, Ks).

reset_counter(K) :- bb_put(K, 0).

inc(K) :-
    bb_get(K, V0),
    V is V0 + 1,
    bb_put(K, V).

stats(stats(P, A, S, M, I, R, B, D)) :-
    bb_get(proc, P), bb_get(att, A), bb_get(succ, S), bb_get(merges, M),
    bb_get(ins, I), bb_get(rem, R), bb_get(br, B), bb_get(prod, D).

% ----------------------------------------------------------- union-find

empty_uf(uf(P, S, Mi, Me, false)) :-
    empty_assoc(P), empty_assoc(S), empty_assoc(Mi), empty_assoc(Me).

contra(uf(_, _, _, _, true)).

set_contra(uf(P, S, Mi, Me, _), uf(P, S, Mi, Me, true)).

find(UF, V, R, P) :-
    UF = uf(Par, _, _, _, _),
    (   get_assoc(V, Par, Q-PV)
    ->  find(UF, Q, R, P0),
        P is xor(P0, PV)
    ;   R = V,
        P = 0
    ).

size(uf(_, S, _, _, _), R, N) :-
    ( get_assoc(R, S, N0) -> N = N0 ; N = 1 ).

min(uf(_, _, Mi, _, _), R, M) :-
    ( get_assoc(R, Mi, M0) -> M = M0 ; M = R ).

members(uf(_, _, _, Me, _), R, Ms) :-
    ( get_assoc(R, Me, Ms0) -> Ms = Ms0 ; Ms = [R] ).

related(UF, A, B, P) :-
    find(UF, A, R, PA),
    find(UF, B, R, PB),
    P is xor(PA, PB).

determined(UF, V) :- related(UF, V, 0, _).

class_min(UF, V, M) :-
    find(UF, V, R, _),
    min(UF, R, M).

% union(+UF0, +LitA, +LitB, -Result, -UF, -Scan): asserts LitA ≡ LitB.
% Result is merged, redundant or contradiction. Scan holds the nodes of
% the scan class (SPEC-Z3) after a merge.
union(UF0, _, _, contradiction, UF0, []) :-
    contra(UF0),
    !.
union(UF0, LA, LB, Res, UF, Scan) :-
    A is LA >> 1,
    B is LB >> 1,
    P is xor(LA /\ 1, LB /\ 1),
    find(UF0, A, RA, PA),
    find(UF0, B, RB, PB),
    (   RA =:= RB
    ->  Scan = [],
        (   xor(PA, PB) =:= P
        ->  Res = redundant, UF = UF0
        ;   Res = contradiction, set_contra(UF0, UF)
        )
    ;   Res = merged,
        find(UF0, 0, RT, _),
        size(UF0, RA, SA), size(UF0, RB, SB),
        min(UF0, RA, MA), min(UF0, RB, MB),
        (   RA =:= RT -> ScanR = RB
        ;   RB =:= RT -> ScanR = RA
        ;   ( SA < SB ; SA =:= SB, MA > MB ) -> ScanR = RA
        ;   ScanR = RB
        ),
        members(UF0, ScanR, Scan),
        (   SA < SB -> Child = RA, Root = RB
        ;   SA > SB -> Child = RB, Root = RA
        ;   ScanR =:= RA -> Child = RA, Root = RB
        ;   Child = RB, Root = RA
        ),
        Link is xor(xor(PA, PB), P),
        S is SA + SB,
        M is min(MA, MB),
        members(UF0, Child, MC),
        members(UF0, Root, MR),
        append(MC, MR, Ms),
        UF0 = uf(Par0, Size0, Min0, Mem0, C),
        put_assoc(Child, Par0, Root-Link, Par),
        put_assoc(Root, Size0, S, Size),
        put_assoc(Root, Min0, M, Min),
        put_assoc(Root, Mem0, Ms, Mem),
        UF = uf(Par, Size, Min, Mem, C)
    ).

% ------------------------------------------------------------- worklist

empty_queue(q([], [], In)) :- empty_assoc(In).

pop(q([T|F], B, In0), T, q(F, B, In)) :-
    !,
    put_assoc(T, In0, false, In).
pop(q([], B, In), T, Q) :-
    B \== [],
    reverse(B, F),
    pop(q(F, [], In), T, Q).

push(T, Q0, Q) :-
    Q0 = q(F, B, In0),
    (   get_assoc(T, In0, true)
    ->  Q = Q0
    ;   put_assoc(T, In0, true, In),
        inc(ins),
        Q = q(F, [T|B], In)
    ).

% Enqueues every triple mentioning a scan-class node, ascending.
enqueue_affected(Scan, Q0, Q) :-
    findall(T, ( member(V, Scan),
                 occ(V, Ts),
                 member(T, Ts) ), Ts0),
    sort(Ts0, Ts),
    foldl(push, Ts, Q0, Q).

assert_eq(UF0, A, B, Res, UF, Q0, Q) :-
    union(UF0, A, B, Res, UF, Scan),
    (   Res == merged
    ->  inc(merges),
        enqueue_affected(Scan, Q0, Q)
    ;   Q = Q0
    ).

% ---------------------------------------------- simple rules (SPEC-S1)

lit_value(Vals, L, X) :-
    N is L >> 1,
    memberchk(N-B, Vals),
    X is xor(B, L /\ 1).

holds(and, Vals, O, A, B) :-
    memberchk(O-XO, Vals),
    lit_value(Vals, A, XA),
    lit_value(Vals, B, XB),
    XO =:= XA /\ XB.
holds(eq, Vals, O, A, B) :-
    memberchk(O-XO, Vals),
    lit_value(Vals, A, XA),
    lit_value(Vals, B, XB),
    ( XA =:= XB -> XO =:= 1 ; XO =:= 0 ).

bits([], []).
bits([N|Ns], [N-B|Vs]) :-
    member(B, [0, 1]),
    bits(Ns, Vs).

% Ordered pairs (I < J) of a sorted node list.
pairs([], []).
pairs([N|Ns], Ps) :-
    findall(N-M, member(M, Ns), Ps0),
    pairs(Ns, Ps1),
    append(Ps0, Ps1, Ps).

consistent(_, [], _).
consistent(UF, [N-M|Ps], Vals) :-
    (   related(UF, N, M, P)
    ->  memberchk(N-XN, Vals),
        memberchk(M-XM, Vals),
        P =:= xor(XN, XM)
    ;   true
    ),
    consistent(UF, Ps, Vals).

% local_closure(+UF, +Triple, -Derived): Derived is `contradiction`, or the
% new equivalences LitA-LitB in ascending node-pair order.
local_closure(UF, t(Kind, O, A, B), Derived) :-
    NA is A >> 1,
    NB is B >> 1,
    sort([0, O, NA, NB], [0|Free]),
    pairs([0|Free], Pairs),
    findall(Vals,
            ( bits(Free, Vs),
              Vals = [0-1|Vs],
              holds(Kind, Vals, O, A, B),
              consistent(UF, Pairs, Vals) ),
            Models),
    (   Models == []
    ->  Derived = contradiction
    ;   findall(LN-LM,
                ( member(N-M, Pairs),
                  \+ related(UF, N, M, _),
                  common_parity(Models, N, M, P),
                  LN is N * 2,
                  LM is M * 2 + P ),
                Derived)
    ).

common_parity([Vals|Vss], N, M, P) :-
    memberchk(N-XN, Vals),
    memberchk(M-XM, Vals),
    P is xor(XN, XM),
    \+ ( member(Vs, Vss),
         memberchk(N-YN, Vs),
         memberchk(M-YM, Vs),
         P =\= xor(YN, YM) ).

% ------------------------------------------------- 0-saturation (§6)

sat0(UF0, Q0, UF) :-
    (   pop(Q0, T, Q1)
    ->  inc(rem),
        inc(proc),
        t(T, Kind, O, A, B),
        local_closure(UF0, t(Kind, O, A, B), D),
        (   D == contradiction
        ->  set_contra(UF0, UF)
        ;   apply_derived(D, UF0, Q1, UF1, Q2),
            (   contra(UF1)
            ->  UF = UF1
            ;   sat0(UF1, Q2, UF)
            )
        )
    ;   UF = UF0
    ).

apply_derived([], UF, Q, UF, Q).
apply_derived([A-B|Ds], UF0, Q0, UF, Q) :-
    (   contra(UF0)
    ->  UF = UF0,
        Q = Q0
    ;   inc(att),
        assert_eq(UF0, A, B, Res, UF1, Q0, Q1),
        ( Res == redundant -> true ; inc(succ) ),
        apply_derived(Ds, UF1, Q1, UF, Q)
    ).

% -------------------------------------- k-saturation and Dilemma (§7, §8)

saturate(0, UF0, Q0, UF) :-
    !,
    sat0(UF0, Q0, UF).
saturate(K, UF0, Q0, UF) :-
    J is K - 1,
    saturate(J, UF0, Q0, UF1),
    passes(J, UF1, UF).

passes(J, UF0, UF) :-
    nnodes(N),
    Last is N - 1,
    pass(1, Last, J, UF0, UF1, false, Changed),
    (   contra(UF1) -> UF = UF1
    ;   Changed == true -> passes(J, UF1, UF)
    ;   UF = UF1
    ).

pass(V, Last, _, UF, UF, C, C) :-
    ( V > Last ; contra(UF) ),
    !.
pass(V, Last, J, UF0, UF, C0, C) :-
    V1 is V + 1,
    (   ( determined(UF0, V) ; class_min(UF0, V, M), M =\= V )
    ->  pass(V1, Last, J, UF0, UF, C0, C)
    ;   check_branch_limit,
        inc(br),
        dilemma(UF0, V, J, UF1, Productive),
        (   Productive == true
        ->  inc(prod),
            C1 = true
        ;   C1 = C0
        ),
        pass(V1, Last, J, UF1, UF, C1, C)
    ).

check_branch_limit :-
    bb_get(max_branches, Max),
    bb_get(br, B),
    (   Max >= 0, B >= Max
    ->  throw(stal_limit(branches))
    ;   true
    ).

% A branch: assert V ≡ C (C is the literal 1 for ⊥ or 0 for ⊤) and
% saturate to depth J from the affected triples.
branch(UF0, V, C, J, UF) :-
    L is V * 2,
    empty_queue(Q0),
    assert_eq(UF0, L, C, _, UF1, Q0, Q),
    saturate(J, UF1, Q, UF).

dilemma(UF, V, J, UFm, Productive) :-
    branch(UF, V, 1, J, R1),
    branch(UF, V, 0, J, R2),
    (   contra(R1) -> UFm = R2, Productive = true
    ;   contra(R2) -> UFm = R1, Productive = true
    ;   meet(UF, R1, R2, UFm, Productive)
    ).

% R1 ⊓ R2 installed into the parent UF by grouping on the SPEC-R6 key.
meet(UF0, R1, R2, UF, Productive) :-
    nnodes(N),
    Last is N - 1,
    findall(k(G1, G2, PK, U, P1),
            ( between(0, Last, U),
              find(R1, U, G1, P1),
              find(R2, U, G2, P2),
              PK is xor(P1, P2) ),
            Keys0),
    sort(Keys0, Keys),
    install(Keys, none, UF0, UF, false, Productive).

install([], _, UF, UF, P, P).
install([k(G1, G2, PK, U, P1)|Ks], Prev, UF0, UF, P0, P) :-
    (   Prev = rep(G1, G2, PK, W, PW)
    ->  Par is xor(PW, P1),
        LW is W * 2,
        LU is U * 2 + Par,
        union(UF0, LW, LU, Res, UF1, _),
        ( Res == merged -> P1x = true ; P1x = P0 ),
        install(Ks, Prev, UF1, UF, P1x, P)
    ;   install(Ks, rep(G1, G2, PK, U, P1), UF0, UF, P0, P)
    ).

% --------------------------------------------------- interface to Rust

% State: the parent relation as [c(Node, Min, Parity), ...], listing only
% nodes with Node =\= Min. Out is r(Classes, Stats), limit(Stats), or
% contradiction(Stats); Classes has the same shape as State.
branch_entry(State, V, C, J, MaxBranches, Out) :-
    reset_counters,
    bb_put(max_branches, MaxBranches),
    empty_uf(UF0),
    foldl(load_class, State, UF0, UF1),
    catch(( branch(UF1, V, C, J, UF),
            stats(S),
            (   contra(UF)
            ->  Out = contradiction(S)
            ;   canonical(UF, Classes),
                Out = r(Classes, S)
            ) ),
          stal_limit(branches),
          ( stats(S), Out = limit(S) )).

load_class(c(N, M, P), UF0, UF) :-
    LM is M * 2,
    LN is N * 2 + P,
    union(UF0, LM, LN, _, UF, _).

canonical(UF, Classes) :-
    nnodes(N),
    Last is N - 1,
    findall(c(V, M, P),
            ( between(0, Last, V),
              find(UF, V, R, PV),
              min(UF, R, M),
              M =\= V,
              find(UF, M, _, PM),
              P is xor(PV, PM) ),
            Classes).
