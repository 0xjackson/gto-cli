import pytest
from gto.cards import Card, parse_board
from gto.equity import equity_vs_hand, equity_vs_range


def c(notation):
    return Card(notation[0], notation[1])


class TestEquityVsHand:
    def test_aa_vs_kk(self):
        result = equity_vs_hand(
            [c("As"), c("Ah")],
            [c("Ks"), c("Kh")],
            simulations=10000,
        )
        assert 0.75 < result.equity < 0.88

    def test_aa_vs_kk_on_flop(self):
        board = parse_board("2s5d8c")
        result = equity_vs_hand(
            [c("As"), c("Ah")],
            [c("Ks"), c("Kh")],
            board=board,
            simulations=10000,
        )
        assert result.equity > 0.85

    def test_coinflip(self):
        result = equity_vs_hand(
            [c("As"), c("Ks")],
            [c("Qh"), c("Qd")],
            simulations=10000,
        )
        assert 0.40 < result.equity < 0.60

    def test_made_hand_vs_draw(self):
        board = parse_board("Ts9s2h")
        result = equity_vs_hand(
            [c("Td"), c("Th")],  # set of tens
            [c("As"), c("Ks")],  # nut flush draw
            board=board,
            simulations=10000,
        )
        assert result.equity > 0.50

    def test_result_string(self):
        result = equity_vs_hand(
            [c("As"), c("Ah")],
            [c("Ks"), c("Kh")],
            simulations=1000,
        )
        assert "Win" in str(result)
        assert "equity" in str(result)


class TestEquityVsRange:
    def test_aa_vs_premium(self):
        result = equity_vs_range(
            [c("As"), c("Ah")],
            ["KK", "QQ", "JJ"],
            simulations=5000,
        )
        assert result.equity > 0.70

    def test_no_valid_combos(self):
        # Hand blocks all combos of the same specific hand
        with pytest.raises(ValueError):
            equity_vs_range(
                [c("As"), c("Ah")],
                ["AsAh"],  # exact combo blocked
                simulations=100,
            )
