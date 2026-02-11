import random
from dataclasses import dataclass

from gto.cards import Card, Deck, RANKS, SUITS, hand_combos
from gto.hand_evaluator import evaluate_hand


@dataclass
class EquityResult:
    win: float
    tie: float
    lose: float
    simulations: int

    @property
    def equity(self) -> float:
        return self.win + self.tie / 2

    def __str__(self) -> str:
        return f"Win {self.win:.1%} | Tie {self.tie:.1%} | Lose {self.lose:.1%} (equity: {self.equity:.1%})"


def equity_vs_hand(
    hand1: list[Card],
    hand2: list[Card],
    board: list[Card] | None = None,
    simulations: int = 50000,
) -> EquityResult:
    if board is None:
        board = []
    dead = set(hand1 + hand2 + board)
    remaining_deck = [Card(r, s) for r in RANKS for s in SUITS if Card(r, s) not in dead]
    cards_needed = 5 - len(board)

    wins = ties = losses = 0
    for _ in range(simulations):
        runout = random.sample(remaining_deck, cards_needed)
        full_board = board + runout
        r1 = evaluate_hand(hand1, full_board)
        r2 = evaluate_hand(hand2, full_board)
        if r1 > r2:
            wins += 1
        elif r1 == r2:
            ties += 1
        else:
            losses += 1

    total = wins + ties + losses
    return EquityResult(
        win=wins / total,
        tie=ties / total,
        lose=losses / total,
        simulations=total,
    )


def equity_vs_range(
    hand: list[Card],
    villain_range: list[str],
    board: list[Card] | None = None,
    simulations: int = 50000,
) -> EquityResult:
    if board is None:
        board = []
    dead = set(hand + board)

    # Build all valid villain combos
    all_combos = []
    for notation in villain_range:
        for c1, c2 in hand_combos(notation):
            if c1 not in dead and c2 not in dead:
                all_combos.append([c1, c2])

    if not all_combos:
        raise ValueError("No valid villain combos after removing dead cards")

    wins = ties = losses = 0
    sims_per = max(1, simulations // len(all_combos))

    for villain_hand in all_combos:
        combo_dead = dead | set(villain_hand)
        remaining = [Card(r, s) for r in RANKS for s in SUITS if Card(r, s) not in combo_dead]
        cards_needed = 5 - len(board)

        for _ in range(sims_per):
            runout = random.sample(remaining, cards_needed)
            full_board = board + runout
            r1 = evaluate_hand(hand, full_board)
            r2 = evaluate_hand(villain_hand, full_board)
            if r1 > r2:
                wins += 1
            elif r1 == r2:
                ties += 1
            else:
                losses += 1

    total = wins + ties + losses
    return EquityResult(
        win=wins / total,
        tie=ties / total,
        lose=losses / total,
        simulations=total,
    )
