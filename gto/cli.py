import click
from rich.console import Console
from rich.table import Table
from rich import box

from gto.display import (
    console, range_grid, equity_bar, decision_panel, print_action,
    print_section, print_error, board_display, odds_table, action_style,
)

POSITIONS_6MAX = ["UTG", "HJ", "CO", "BTN", "SB", "BB"]
POSITIONS_9MAX = ["UTG", "UTG1", "UTG2", "MP", "HJ", "CO", "BTN", "SB", "BB"]


def validate_position(pos: str, table_size: str) -> str:
    pos = pos.upper()
    valid = POSITIONS_9MAX if table_size == "9max" else POSITIONS_6MAX
    if pos not in valid:
        raise click.BadParameter(
            f"Invalid position '{pos}'. Valid: {', '.join(valid)}")
    return pos


@click.group()
@click.version_option(version="1.0.0", prog_name="gto")
def main():
    """GTO Poker Toolkit — preflop ranges, equity, odds, and strategy."""
    pass


@main.command()
@click.argument("position")
@click.option("--table", "-t", default="6max", type=click.Choice(["6max", "9max"]),
              help="Table format.")
@click.option("--vs", default=None, help="Villain position (for vs_RFI / vs_3bet).")
@click.option("--situation", "-s", default="RFI",
              type=click.Choice(["RFI", "vs_RFI", "vs_3bet", "bb_defense"]),
              help="Preflop situation.")
def range(position, table, vs, situation):
    """Show preflop opening range for a position.

    Examples:

      gto range BTN

      gto range CO --table 9max

      gto range BB --vs BTN --situation vs_RFI
    """
    from gto.preflop import get_rfi_range, get_rfi_pct, get_vs_rfi_range, get_vs_3bet_range, get_bb_defense
    from gto.ranges import total_combos, range_pct

    try:
        position = validate_position(position, table)
    except click.BadParameter as e:
        print_error(str(e))
        return

    if situation == "RFI":
        hands = get_rfi_range(position, table)
        pct = get_rfi_pct(position, table)
        title = f"{position} RFI Range ({table})"

        console.print()
        console.print(range_grid(hands, title))
        console.print()
        console.print(f"  [bold]{len(hands)}[/bold] hands | "
                      f"[bold]{total_combos(hands)}[/bold] combos | "
                      f"[bold]{pct}%[/bold] of hands")
        console.print()

    elif situation == "vs_RFI":
        if not vs:
            print_error("--vs required for vs_RFI situation")
            return
        vs = validate_position(vs, table)
        result = get_vs_rfi_range(position, vs, table)

        console.print()
        console.print(f"[bold]{position} vs {vs} Open ({table})[/bold]\n")

        if result.three_bet:
            console.print(f"  [bold red]3-Bet:[/bold red] {', '.join(result.three_bet)}")
            console.print(f"        {total_combos(result.three_bet)} combos ({range_pct(result.three_bet):.1f}%)")
        if result.call:
            console.print(f"  [bold green]Call:[/bold green]  {', '.join(result.call)}")
            console.print(f"        {total_combos(result.call)} combos ({range_pct(result.call):.1f}%)")
        console.print(f"  [dim]Fold:[/dim]  everything else")

        all_hands = result.three_bet + result.call
        console.print()
        console.print(range_grid(all_hands, f"{position} vs {vs}"))
        console.print()

    elif situation == "vs_3bet":
        if not vs:
            print_error("--vs required for vs_3bet situation")
            return
        vs = validate_position(vs, table)
        result = get_vs_3bet_range(position, vs, table)

        console.print()
        console.print(f"[bold]{position} vs {vs} 3-Bet ({table})[/bold]\n")

        if result.four_bet:
            console.print(f"  [bold red]4-Bet:[/bold red] {', '.join(result.four_bet)}")
            console.print(f"        {total_combos(result.four_bet)} combos")
        if result.call:
            console.print(f"  [bold green]Call:[/bold green]  {', '.join(result.call)}")
            console.print(f"        {total_combos(result.call)} combos")
        console.print(f"  [dim]Fold:[/dim]  everything else")
        console.print()

    elif situation == "bb_defense":
        if not vs:
            print_error("--vs required for bb_defense situation")
            return
        vs = validate_position(vs, table)
        result = get_bb_defense(vs, table)

        console.print()
        console.print(f"[bold]BB Defense vs {vs} ({table})[/bold]\n")

        if result.three_bet:
            console.print(f"  [bold red]3-Bet:[/bold red] {', '.join(result.three_bet)}")
            console.print(f"        {total_combos(result.three_bet)} combos")
        if result.call:
            console.print(f"  [bold green]Call:[/bold green]  {', '.join(result.call)}")
            console.print(f"        {total_combos(result.call)} combos")
        console.print(f"  [dim]Fold:[/dim]  everything else")

        all_hands = result.three_bet + result.call
        console.print()
        console.print(range_grid(all_hands, f"BB vs {vs}"))
        console.print()


@main.command()
@click.argument("hand1")
@click.argument("versus", required=False, default=None)
@click.argument("hand2", required=False, default=None)
@click.option("--board", "-b", default=None, help="Board cards (e.g., AsKdQh).")
@click.option("--sims", "-n", default=30000, help="Number of simulations.")
def equity(hand1, versus, hand2, board, sims):
    """Calculate equity between two hands or hand vs range.

    Examples:

      gto equity AhAs vs KsKd

      gto equity AhAs vs KK

      gto equity AhAs vs KsKd --board AsKd5c
    """
    from gto.cards import parse_card, parse_board as pb
    from gto.equity import equity_vs_hand, equity_vs_range

    if hand2 is None and versus is not None and versus.lower() != "vs":
        hand2 = versus
        versus = None

    if hand2 is None:
        print_error("Usage: gto equity <hand1> vs <hand2|range>")
        return

    board_cards = pb(board) if board else None

    try:
        h1 = [parse_card(hand1[i:i+2]) for i in range(0, len(hand1), 2)]
    except (ValueError, IndexError):
        print_error(f"Invalid hand: {hand1}")
        return

    # Try parsing hand2 as specific cards first
    is_range = False
    try:
        if len(hand2) == 4:
            h2 = [parse_card(hand2[i:i+2]) for i in range(0, len(hand2), 2)]
        else:
            is_range = True
    except (ValueError, IndexError):
        is_range = True

    console.print()
    board_str = ""
    if board_cards:
        board_str = f" on {board_display(board_cards)}"

    if is_range:
        from gto.ranges import parse_range
        villain_range = parse_range(hand2)
        console.print(f"  [bold]{hand1}[/bold] vs [bold]{hand2}[/bold]{board_str}")
        console.print(f"  Running {sims:,} simulations...\n")
        result = equity_vs_range(h1, villain_range, board_cards, sims)
    else:
        console.print(f"  [bold]{hand1}[/bold] vs [bold]{hand2}[/bold]{board_str}")
        console.print(f"  Running {sims:,} simulations...\n")
        result = equity_vs_hand(h1, h2, board_cards, sims)

    console.print(f"  Hero:  {equity_bar(result.equity)}")
    console.print(f"  Villain: {equity_bar(1 - result.equity)}")
    console.print()

    t = Table(box=box.ROUNDED, show_header=False, expand=False)
    t.add_column("", style="bold")
    t.add_column("", justify="right")
    t.add_row("Win", f"{result.win:.1%}")
    t.add_row("Tie", f"{result.tie:.1%}")
    t.add_row("Lose", f"{result.lose:.1%}")
    t.add_row("Equity", f"[bold]{result.equity:.1%}[/bold]")
    t.add_row("Sims", f"{result.simulations:,}")
    console.print(t)
    console.print()


@main.command()
@click.argument("pot", type=float)
@click.argument("bet", type=float)
@click.option("--equity", "-e", "equity_val", type=float, default=None,
              help="Your equity (0-1) to calculate EV.")
@click.option("--implied", "-i", "future", type=float, default=None,
              help="Expected future winnings for implied odds.")
def odds(pot, bet, equity_val, future):
    """Calculate pot odds, EV, and implied odds.

    Examples:

      gto odds 100 50

      gto odds 100 50 --equity 0.35

      gto odds 100 50 --implied 200
    """
    from gto.math_engine import pot_odds, ev, implied_odds

    needed = pot_odds(pot, bet)

    console.print()
    t = odds_table(pot, bet, needed)

    if equity_val is not None:
        ev_val = ev(equity_val, pot, bet)
        color = "green" if ev_val >= 0 else "red"
        t.add_row("Your Equity", f"{equity_val:.1%}")
        t.add_row("EV of Call", f"[{color}]${ev_val:.2f}[/{color}]")
        verdict = "[bold green]CALL[/bold green]" if ev_val >= 0 else "[bold red]FOLD[/bold red]"
        t.add_row("Verdict", verdict)

    if future is not None:
        imp = implied_odds(pot, bet, future)
        t.add_row("Implied Odds", f"{imp:.1%}")
        t.add_row("Future Value", f"${future:.0f}")

    console.print(t)
    console.print()


@main.command()
@click.argument("cards")
def board(cards):
    """Analyze board texture.

    Examples:

      gto board AsKd7c

      gto board Ts9s8d

      gto board AsKdQhJs
    """
    from gto.cards import parse_board as pb
    from gto.postflop import analyze_board, cbet_recommendation

    try:
        board_cards = pb(cards)
    except ValueError as e:
        print_error(str(e))
        return

    texture = analyze_board(board_cards)

    console.print()
    console.print(f"  Board: {board_display(board_cards)}")
    console.print()

    t = Table(box=box.ROUNDED, show_header=False, expand=False)
    t.add_column("", style="bold")
    t.add_column("")
    t.add_row("Texture", texture.category)
    t.add_row("Wetness", texture.wetness.upper())
    t.add_row("High Card", texture.high_card)
    t.add_row("Paired", "Yes" if texture.is_paired else "No")
    t.add_row("Flush Draw", "Yes" if texture.flush_draw_possible else "No")
    t.add_row("Straight Draw", "Yes" if texture.straight_draw_possible else "No")
    if texture.draws:
        t.add_row("Draws", ", ".join(texture.draws))
    console.print(t)
    console.print()

    cbet_ip = cbet_recommendation(texture, "IP")
    cbet_oop = cbet_recommendation(texture, "OOP")

    console.print("[bold]C-Bet Guidance:[/bold]")
    console.print(f"  IP:  {cbet_ip.frequency:.0%} frequency, {cbet_ip.sizing} — {cbet_ip.reasoning}")
    console.print(f"  OOP: {cbet_oop.frequency:.0%} frequency, {cbet_oop.sizing} — {cbet_oop.reasoning}")
    console.print()


@main.command()
@click.argument("hand")
@click.option("--position", "-p", required=True, help="Your position.")
@click.option("--board", "-b", default=None, help="Board cards.")
@click.option("--pot", type=float, default=None, help="Current pot size.")
@click.option("--stack", type=float, default=None, help="Effective stack size.")
@click.option("--vs", default=None, help="Villain position.")
@click.option("--situation", "-s", default="RFI",
              type=click.Choice(["RFI", "vs_RFI", "vs_3bet"]),
              help="Preflop situation.")
@click.option("--table", "-t", default="6max", type=click.Choice(["6max", "9max"]))
@click.option("--players", type=int, default=2, help="Number of players in pot.")
@click.option("--street", default=None, type=click.Choice(["flop", "turn", "river"]))
@click.option("--strength", default=None,
              type=click.Choice(["nuts", "very_strong", "strong", "medium", "draw", "bluff", "weak"]),
              help="Hand strength category (for postflop).")
def action(hand, position, board, pot, stack, vs, situation, table, players, street, strength):
    """Full decision advisor — preflop and postflop.

    Examples:

      gto action AKs -p BTN

      gto action AKs -p CO --vs UTG --situation vs_RFI

      gto action AKs -p BTN -b AsKd7c --pot 100 --stack 500 --street flop --strength strong
    """
    from gto.preflop import preflop_action
    from gto.postflop import analyze_board, street_strategy
    from gto.math_engine import spr as calc_spr
    from gto.multiway import multiway_cbet, multiway_range_adjustment
    from gto.cards import parse_board as pb

    position = validate_position(position, table)

    console.print()
    console.print(f"  [bold]Hand:[/bold] {hand}  [bold]Position:[/bold] {position}  [bold]Table:[/bold] {table}")

    if board is None:
        result = preflop_action(hand, position, situation, vs, table)
        console.print()
        styled = f"[{action_style(result.action)}]{result.action}[/{action_style(result.action)}]"
        console.print(f"  Action: {styled}")
        console.print(f"  {result.detail}")
        console.print()
        return

    board_cards = pb(board)
    texture = analyze_board(board_cards)
    console.print(f"  [bold]Board:[/bold] {board_display(board_cards)}")

    if pot and stack:
        spr_result = calc_spr(stack, pot)
        console.print(f"  [bold]SPR:[/bold] {spr_result}")
        console.print(f"  {spr_result.guidance}")

    if players > 2:
        adj = multiway_range_adjustment(players)
        console.print(f"  [bold]Multiway ({players} players):[/bold] {adj}")

    console.print(f"  [bold]Texture:[/bold] {texture.category}")

    if strength and street and pot and stack:
        strat = street_strategy(strength, texture, pot, stack, "IP" if position in ("BTN", "CO") else "OOP", street)
        console.print()
        styled = f"[{action_style(strat.action)}]{strat.action}[/{action_style(strat.action)}]"
        console.print(f"  Action: {styled}  {strat.sizing}")
        console.print(f"  {strat.reasoning}")

    console.print()


@main.command()
@click.argument("pot", type=float)
@click.argument("bet", type=float)
@click.option("--players", "-n", type=int, default=2, help="Number of players.")
def mdf(pot, bet, players):
    """Calculate minimum defense frequency.

    Examples:

      gto mdf 100 50

      gto mdf 100 75 --players 3
    """
    from gto.math_engine import mdf as calc_mdf
    from gto.multiway import multiway_defense_freq

    console.print()
    base = calc_mdf(bet, pot)
    console.print(f"  [bold]MDF:[/bold] {base:.1%}")
    console.print(f"  You must defend at least {base:.1%} of your range")
    console.print(f"  to prevent villain from profiting with any two cards.")

    if players > 2:
        per_player = multiway_defense_freq(players, bet, pot)
        console.print(f"\n  [bold]Multiway ({players} players):[/bold]")
        console.print(f"  Per-player defense: {per_player:.1%}")
    console.print()


@main.command("spr")
@click.argument("stack_size", type=float)
@click.argument("pot_size", type=float)
def spr_cmd(stack_size, pot_size):
    """Analyze stack-to-pot ratio.

    Examples:

      gto spr 500 100

      gto spr 200 100
    """
    from gto.math_engine import spr

    result = spr(stack_size, pot_size)

    console.print()
    t = Table(box=box.ROUNDED, show_header=False, expand=False)
    t.add_column("", style="bold")
    t.add_column("")
    t.add_row("Stack", f"${stack_size:.0f}")
    t.add_row("Pot", f"${pot_size:.0f}")
    t.add_row("SPR", f"[bold]{result.ratio:.1f}[/bold]")
    t.add_row("Zone", result.zone.upper())
    t.add_row("Guidance", result.guidance)
    console.print(t)
    console.print()


@main.command()
@click.argument("range_str")
def combos(range_str):
    """Count combos in a range.

    Examples:

      gto combos "AA,KK,QQ,AKs"

      gto combos "TT+"

      gto combos "ATs-AKs"
    """
    from gto.ranges import parse_range, total_combos, combo_count, range_pct

    hands = parse_range(range_str)

    console.print()
    t = Table(box=box.ROUNDED, show_header=True)
    t.add_column("Hand", style="bold")
    t.add_column("Combos", justify="right")

    for h in hands:
        t.add_row(h, str(combo_count(h)))

    t.add_section()
    total = total_combos(hands)
    pct = range_pct(hands)
    t.add_row("[bold]Total[/bold]", f"[bold]{total}[/bold]")
    t.add_row("[bold]% of hands[/bold]", f"[bold]{pct:.1f}%[/bold]")
    console.print(t)
    console.print()
    console.print(range_grid(hands, range_str))
    console.print()


@main.command()
@click.argument("pot", type=float)
@click.argument("bet", type=float)
def bluff(pot, bet):
    """Calculate bluff-to-value ratio and fold equity needed.

    Examples:

      gto bluff 100 75

      gto bluff 100 100
    """
    from gto.math_engine import bluff_to_value_ratio, fold_equity, break_even_pct

    ratio = bluff_to_value_ratio(bet, pot)
    be_pct = break_even_pct(pot, bet)

    console.print()
    t = Table(box=box.ROUNDED, show_header=False, expand=False)
    t.add_column("", style="bold")
    t.add_column("", justify="right")
    t.add_row("Pot", f"${pot:.0f}")
    t.add_row("Bet", f"${bet:.0f}")
    t.add_row("Bluff Ratio", f"{ratio:.1%}")
    t.add_row("Break-Even", f"{be_pct:.1%}")
    console.print(t)

    console.print(f"\n  For every [bold]1[/bold] value bet, you can bluff [bold]{ratio/(1-ratio):.2f}[/bold] times.")
    console.print(f"  Villain needs to fold [bold]{be_pct:.1%}[/bold] for a 0 EV bluff.")
    console.print()


if __name__ == "__main__":
    main()
