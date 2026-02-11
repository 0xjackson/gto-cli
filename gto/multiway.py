from dataclasses import dataclass


def multiway_defense_freq(num_players: int, bet_size: float, pot: float) -> float:
    if pot <= 0 or num_players < 2:
        raise ValueError("Invalid pot or player count")
    base_mdf = pot / (pot + bet_size)
    # Each additional player means less individual defending needed
    per_player = 1 - (1 - base_mdf) ** (1 / (num_players - 1))
    return per_player


def multiway_cbet(num_players: int, board_wetness: str, position: str = "IP") -> "MultiwayBetAdvice":
    ip = position.upper() == "IP"

    if num_players >= 4:
        return MultiwayBetAdvice(
            frequency=0.15,
            sizing="50-66% pot",
            reasoning="4+ players — rarely c-bet, need strong hands or strong draws",
        )

    if num_players == 3:
        if board_wetness == "dry":
            freq = 0.35 if ip else 0.20
            return MultiwayBetAdvice(
                frequency=freq,
                sizing="33% pot",
                reasoning="3-way dry board — small sizing, reduced frequency",
            )
        if board_wetness == "wet":
            freq = 0.25 if ip else 0.15
            return MultiwayBetAdvice(
                frequency=freq,
                sizing="66-75% pot",
                reasoning="3-way wet board — only strong hands/draws, larger sizing",
            )
        freq = 0.30 if ip else 0.20
        return MultiwayBetAdvice(
            frequency=freq,
            sizing="50% pot",
            reasoning="3-way medium texture — selective betting",
        )

    # 2 players is heads-up, standard advice
    if board_wetness == "dry":
        freq = 0.65 if ip else 0.45
        return MultiwayBetAdvice(frequency=freq, sizing="33% pot",
                                 reasoning="Heads-up dry — standard c-bet")
    if board_wetness == "wet":
        freq = 0.45 if ip else 0.30
        return MultiwayBetAdvice(frequency=freq, sizing="66-75% pot",
                                 reasoning="Heads-up wet — polarized sizing")
    freq = 0.55 if ip else 0.40
    return MultiwayBetAdvice(frequency=freq, sizing="50% pot",
                             reasoning="Heads-up medium texture — balanced")


@dataclass
class MultiwayBetAdvice:
    frequency: float
    sizing: str
    reasoning: str


def multiway_sizing(num_players: int, board_wetness: str) -> str:
    if num_players >= 4:
        return "66-75% pot (punish draws, narrow ranges)"
    if num_players == 3:
        if board_wetness == "dry":
            return "25-33% pot"
        return "50-66% pot"
    if board_wetness == "dry":
        return "25-33% pot"
    if board_wetness == "wet":
        return "66-75% pot"
    return "50% pot"


def multiway_range_adjustment(num_players: int) -> str:
    if num_players >= 4:
        return "Tighten significantly. Need top 10-15% of range to continue. Draws need near-nut quality."
    if num_players == 3:
        return "Tighten moderately. Top pair needs good kicker. Draws should have nut potential."
    return "Standard heads-up ranges apply."
