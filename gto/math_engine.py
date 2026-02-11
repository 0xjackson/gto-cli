from dataclasses import dataclass


def pot_odds(pot: float, bet: float) -> float:
    if pot <= 0 or bet <= 0:
        raise ValueError("Pot and bet must be positive")
    return bet / (pot + bet + bet)


def implied_odds(pot: float, bet: float, expected_future: float) -> float:
    if bet <= 0:
        raise ValueError("Bet must be positive")
    return bet / (pot + bet + bet + expected_future)


def reverse_implied_odds(pot: float, bet: float, risk: float) -> float:
    if bet <= 0:
        raise ValueError("Bet must be positive")
    return (bet + risk) / (pot + bet + bet + risk)


def ev(equity: float, pot: float, bet: float) -> float:
    win_amount = pot + bet
    return equity * win_amount - (1 - equity) * bet


def mdf(bet_size: float, pot_size: float) -> float:
    if pot_size <= 0:
        raise ValueError("Pot must be positive")
    return pot_size / (pot_size + bet_size)


def fold_equity(fold_pct: float, pot: float, bet: float) -> float:
    return fold_pct * pot - (1 - fold_pct) * bet


def spr(stack: float, pot: float) -> "SPRResult":
    if pot <= 0:
        raise ValueError("Pot must be positive")
    ratio = stack / pot
    if ratio <= 4:
        zone = "low"
        guidance = "Commit with top pair+. All-in pressure is standard."
    elif ratio <= 10:
        zone = "medium"
        guidance = "Two pair+ for stacking. One pair hands play cautiously."
    else:
        zone = "high"
        guidance = "Need very strong hands to stack off. Implied odds matter most."
    return SPRResult(ratio=ratio, zone=zone, guidance=guidance)


@dataclass
class SPRResult:
    ratio: float
    zone: str
    guidance: str

    def __str__(self) -> str:
        return f"SPR {self.ratio:.1f} ({self.zone})"


def bluff_to_value_ratio(bet_size: float, pot_size: float) -> float:
    if pot_size <= 0:
        raise ValueError("Pot must be positive")
    return bet_size / (pot_size + bet_size)


def break_even_pct(pot: float, bet: float) -> float:
    if pot + bet <= 0:
        raise ValueError("Total pot must be positive")
    return bet / (pot + bet + bet)


def effective_stack(stacks: list[float]) -> float:
    if len(stacks) < 2:
        raise ValueError("Need at least 2 stacks")
    sorted_stacks = sorted(stacks)
    return sorted_stacks[-2]
