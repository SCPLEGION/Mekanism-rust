//! The Induction Matrix (`MatrixMultiblockData`): a hollow box of Induction Casing and Ports, 3 to 18
//! blocks on each side, whose inside holds Induction Cells (storage) and Providers (transfer rate).

pub const MIN_SIZE: i32 = 3;
pub const MAX_SIZE: i32 = 18;

/// `InductionCellTier`: FE a cell stores.
pub fn cell(block: &str) -> Option<u64> {
    Some(
        match block
            .strip_prefix("mekanism:")?
            .strip_suffix("_induction_cell")?
        {
            "basic" => 8_000_000_000,
            "advanced" => 64_000_000_000,
            "elite" => 512_000_000_000,
            "ultimate" => 4_000_000_000_000,
            _ => return None,
        },
    )
}

/// `InductionProviderTier`: FE a tick a provider adds to the matrix's transfer rate.
pub fn provider(block: &str) -> Option<u64> {
    Some(
        match block
            .strip_prefix("mekanism:")?
            .strip_suffix("_induction_provider")?
        {
            "basic" => 256_000,
            "advanced" => 2_048_000,
            "elite" => 16_384_000,
            "ultimate" => 131_072_000,
            _ => return None,
        },
    )
}

pub fn is_frame(block: &str) -> bool {
    matches!(
        block,
        "mekanism:induction_casing" | "mekanism:induction_port"
    )
}

/// What a formed matrix holds and moves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Matrix {
    pub capacity: u64,
    pub rate: u64,
    /// How many cells and providers it holds (`getCellCount`, `getProviderCount`).
    pub cells: u32,
    pub providers: u32,
}

impl Matrix {
    /// Adds an inside block: a cell or provider counts, air is allowed, anything else breaks the matrix.
    pub fn add_inside(&mut self, block: &str) -> bool {
        if let Some(c) = cell(block) {
            self.capacity = self.capacity.saturating_add(c);
            self.cells += 1;
        } else if let Some(p) = provider(block) {
            self.rate = self.rate.saturating_add(p);
            self.providers += 1;
        } else if block != "minecraft:air" {
            return false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_and_providers_add_up() {
        let mut m = Matrix::default();
        assert!(m.add_inside("mekanism:basic_induction_cell"));
        assert!(m.add_inside("mekanism:ultimate_induction_cell"));
        assert!(m.add_inside("mekanism:advanced_induction_provider"));
        assert!(m.add_inside("minecraft:air"));
        assert!(!m.add_inside("minecraft:stone"));
        assert_eq!(
            m,
            Matrix {
                capacity: 4_008_000_000_000,
                rate: 2_048_000,
                cells: 2,
                providers: 1,
            }
        );
        assert!(is_frame("mekanism:induction_port") && !is_frame("mekanism:basic_induction_cell"));
    }
}
