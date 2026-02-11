from rich.console import Console
from rich.table import Table
from rich.panel import Panel
from rich.text import Text
from rich import box

from gto.cards import RANKS, RANK_VALUES

console = Console()

RANGE_GRID_RANKS = list(reversed(RANKS))  # A, K, Q, ... 2


def range_grid(hands_in_range: list[str], title: str = "Range") -> Table:
    in_range = set(hands_in_range)
    table = Table(title=title, box=box.SIMPLE_HEAVY, show_header=True,
                  header_style="bold", padding=(0, 1))
    table.add_column("", style="bold", width=3)
    for r in RANGE_GRID_RANKS:
        table.add_column(r, width=4, justify="center")

    for i, r1 in enumerate(RANGE_GRID_RANKS):
        cells = [f"[bold]{r1}[/bold]"]
        for j, r2 in enumerate(RANGE_GRID_RANKS):
            if i == j:
                hand = f"{r1}{r2}"
            elif i < j:
                hand = f"{r1}{r2}s"
            else:
                hand = f"{r2}{r1}o"

            if hand in in_range:
                cells.append(f"[bold green]{hand}[/bold green]")
            else:
                cells.append(f"[dim]{hand}[/dim]")
        table.add_row(*cells)
    return table


def equity_bar(equity: float, width: int = 30) -> str:
    filled = int(equity * width)
    bar = "\u2588" * filled + "\u2591" * (width - filled)
    pct = f"{equity:.1%}"
    if equity >= 0.6:
        return f"[green]{bar}[/green] {pct}"
    if equity >= 0.4:
        return f"[yellow]{bar}[/yellow] {pct}"
    return f"[red]{bar}[/red] {pct}"


def decision_panel(title: str, items: dict[str, str], style: str = "cyan") -> Panel:
    text = Text()
    for key, value in items.items():
        text.append(f"{key}: ", style="bold")
        text.append(f"{value}\n")
    return Panel(text, title=title, border_style=style, expand=False)


def action_style(action: str) -> str:
    action = action.upper()
    if action in ("RAISE", "3BET", "4BET", "BET"):
        return "bold red"
    if action == "CALL":
        return "bold green"
    if action == "FOLD":
        return "bold dim"
    if "CHECK" in action:
        return "bold yellow"
    return "bold"


def print_action(action: str, detail: str = ""):
    styled = f"[{action_style(action)}]{action}[/{action_style(action)}]"
    if detail:
        console.print(f"  {styled}  {detail}")
    else:
        console.print(f"  {styled}")


def print_section(title: str, content: str):
    console.print(f"\n[bold cyan]{title}[/bold cyan]")
    console.print(f"  {content}")


def print_error(msg: str):
    console.print(f"[bold red]Error:[/bold red] {msg}")


def print_success(msg: str):
    console.print(f"[bold green]{msg}[/bold green]")


def board_display(cards_str: list[str]) -> str:
    parts = []
    suit_colors = {"s": "white", "h": "red", "d": "blue", "c": "green"}
    suit_symbols = {"s": "\u2660", "h": "\u2665", "d": "\u2666", "c": "\u2663"}
    for card in cards_str:
        rank = card.rank if hasattr(card, "rank") else card[0]
        suit = card.suit if hasattr(card, "suit") else card[1]
        color = suit_colors.get(suit, "white")
        symbol = suit_symbols.get(suit, suit)
        parts.append(f"[{color}]{rank}{symbol}[/{color}]")
    return " ".join(parts)


def odds_table(pot: float, bet: float, equity_needed: float, ev_value: float = None) -> Table:
    table = Table(box=box.ROUNDED, show_header=False, expand=False)
    table.add_column("Metric", style="bold")
    table.add_column("Value", justify="right")

    table.add_row("Pot", f"${pot:.0f}")
    table.add_row("Bet", f"${bet:.0f}")
    table.add_row("Pot Odds", f"{equity_needed:.1%}")
    table.add_row("To Call", f"${bet:.0f}")
    table.add_row("Total Pot", f"${pot + bet + bet:.0f}")
    if ev_value is not None:
        color = "green" if ev_value >= 0 else "red"
        table.add_row("EV", f"[{color}]${ev_value:.2f}[/{color}]")
    return table
