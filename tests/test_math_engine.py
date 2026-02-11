import pytest
from gto.math_engine import (
    pot_odds, implied_odds, reverse_implied_odds, ev, mdf,
    fold_equity, spr, bluff_to_value_ratio, break_even_pct, effective_stack,
)


class TestPotOdds:
    def test_basic(self):
        result = pot_odds(100, 50)
        assert abs(result - 0.25) < 0.001

    def test_half_pot(self):
        result = pot_odds(100, 50)
        assert abs(result - 0.25) < 0.001

    def test_full_pot(self):
        result = pot_odds(100, 100)
        assert abs(result - 1/3) < 0.001

    def test_invalid(self):
        with pytest.raises(ValueError):
            pot_odds(0, 50)


class TestImpliedOdds:
    def test_basic(self):
        result = implied_odds(100, 50, 100)
        assert result < pot_odds(100, 50)

    def test_zero_future(self):
        assert abs(implied_odds(100, 50, 0) - pot_odds(100, 50)) < 0.001


class TestReverseImpliedOdds:
    def test_basic(self):
        result = reverse_implied_odds(100, 50, 100)
        assert result > pot_odds(100, 50)


class TestEV:
    def test_positive_ev(self):
        result = ev(0.5, 100, 50)
        assert result > 0

    def test_break_even(self):
        equity = pot_odds(100, 50)
        result = ev(equity, 100, 50)
        assert abs(result) < 0.01

    def test_negative_ev(self):
        result = ev(0.1, 100, 100)
        assert result < 0


class TestMDF:
    def test_half_pot(self):
        result = mdf(50, 100)
        assert abs(result - 2/3) < 0.001

    def test_full_pot(self):
        result = mdf(100, 100)
        assert abs(result - 0.5) < 0.001

    def test_overbet(self):
        result = mdf(200, 100)
        assert abs(result - 1/3) < 0.001


class TestFoldEquity:
    def test_profitable_bluff(self):
        result = fold_equity(0.6, 100, 75)
        assert result > 0

    def test_unprofitable_bluff(self):
        result = fold_equity(0.2, 100, 75)
        assert result < 0


class TestSPR:
    def test_low(self):
        result = spr(200, 100)
        assert result.zone == "low"
        assert abs(result.ratio - 2.0) < 0.01

    def test_medium(self):
        result = spr(700, 100)
        assert result.zone == "medium"

    def test_high(self):
        result = spr(1500, 100)
        assert result.zone == "high"

    def test_str(self):
        result = spr(200, 100)
        assert "2.0" in str(result)


class TestBluffToValueRatio:
    def test_half_pot(self):
        result = bluff_to_value_ratio(50, 100)
        assert abs(result - 1/3) < 0.001

    def test_full_pot(self):
        result = bluff_to_value_ratio(100, 100)
        assert abs(result - 0.5) < 0.001


class TestBreakEvenPct:
    def test_basic(self):
        result = break_even_pct(100, 50)
        assert abs(result - 0.25) < 0.001


class TestEffectiveStack:
    def test_two_stacks(self):
        assert effective_stack([100, 200]) == 100

    def test_three_stacks(self):
        assert effective_stack([50, 100, 200]) == 100

    def test_invalid(self):
        with pytest.raises(ValueError):
            effective_stack([100])
