import { describe, expect, it } from "vitest";
import { parseFurFile } from "@/core/fur/node";
import { fixtureBytes } from "./fixtures";

describe("Furnace .fur parser", () => {
  it("parses the bundled Game Boy fixture", () => {
    const module = parseFurFile(fixtureBytes("tests/fixtures/flight_school_night_shift.fur"));

    expect(module.formatVersion).toBe(251);
    expect(module.info.name).toBe("flight_school_night_shift");
    expect(module.info.author).toBe("John Oestmann");
    expect(module.info.system).toBe("Game Boy");
    expect(module.info.totalChannels).toBe(4);
    expect(module.info.chips).toHaveLength(1);
    expect(module.info.chips[0]!.chipId).toBe(4);

    expect(module.instruments).toHaveLength(10);
    expect(module.instruments[0]!.name).toBe("Bass 1");
    expect(module.instruments[0]!.insType).toBe(2);
    expect(module.instruments[0]!.gameBoy).not.toBeNull();

    expect(module.wavetables).toHaveLength(3);
    expect(module.wavetables[0]!.width).toBe(32);

    expect(module.subsongs).toHaveLength(1);
    const sub = module.subsongs[0]!;
    expect(sub.patternLength).toBe(64);
    expect(sub.orderLength).toBe(13);
    expect(sub.orders).toHaveLength(4);
    expect(sub.orders[0]).toEqual([0, 1, 2, 3, 5, 10, 11, 12, 13, 6, 7, 8, 9]);

    expect(sub.patterns).toHaveLength(52);
    for (let ch = 0; ch < 4; ch++) {
      expect(sub.patterns.filter((p) => p.channel === ch)).toHaveLength(13);
    }

    const p0 = sub.patterns.find((p) => p.channel === 0 && p.index === 0);
    expect(p0).toBeDefined();
    expect(p0!.rows).toHaveLength(64);
    expect(p0!.rows[0]!.instrument).toBe(0);
    expect(p0!.rows[0]!.note).not.toBeNull();
    expect(p0!.rows[0]!.volume).not.toBeNull();
  });

  it("parses the Furnace 0.6 golden-battletrain fixture with effects", () => {
    const module = parseFurFile(
      fixtureBytes("tests/fixtures/golden-battletrain/06-golden_battletrain.fur"),
    );
    expect(module.formatVersion).toBe(181);
    expect(module.info.name).toBe("6-golden_battletrain");
    expect(module.info.totalChannels).toBe(4);
    expect(module.instruments).toHaveLength(6);
    expect(module.wavetables).toHaveLength(2);

    const song = module.subsongs[0]!;
    expect([song.patternLength, song.orderLength]).toEqual([64, 16]);
    const hasF0 = song.patterns
      .flatMap((p) => p.rows)
      .some((cell) => cell.effects.some((e) => e.effect === 0xf0));
    expect(hasF0).toBe(true);
  });

  it("accepts an already-unwrapped buffer and rejects garbage", () => {
    const raw = fixtureBytes("tests/fixtures/flight_school_night_shift.fur");
    const module = parseFurFile(raw);
    expect(module.info.name).toBe("flight_school_night_shift");

    expect(() => parseFurFile(new Uint8Array([1, 2, 3, 4]))).toThrow();
  });
});
