// Item-to-item machine recipes (mekanism.api.recipes.ItemStackToItemStackRecipe), read from this
// repository's generated data by tools/gen_recipe_table.py into recipe_table.rs.

/// What a recipe takes: an item id, or a tag (`c:ores/osmium`, without `#`), or any of several.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ingredient {
    Item(&'static str),
    Tag(&'static str),
    Any(&'static [Ingredient]),
}

impl Ingredient {
    /// Whether `item` matches; `in_tag(tag, item)` answers tag membership from the game's tags.
    pub fn matches(&self, item: &str, in_tag: &dyn Fn(&str, &str) -> bool) -> bool {
        match self {
            Ingredient::Item(id) => *id == item,
            Ingredient::Tag(tag) => in_tag(tag, item),
            Ingredient::Any(all) => all.iter().any(|i| i.matches(item, in_tag)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemRecipe {
    /// The recipe's id (`mekanism:processing/osmium/dust/from_ore`).
    pub id: &'static str,
    pub input: Ingredient,
    pub input_count: u32,
    pub output: &'static str,
    pub output_count: u32,
}

/// The first recipe of `recipes` that `item` (at least `count` of it) matches, as Mekanism's input
/// cache finds the first match.
pub fn find<'a>(
    recipes: &'a [ItemRecipe],
    item: &str,
    count: u32,
    in_tag: &dyn Fn(&str, &str) -> bool,
) -> Option<&'a ItemRecipe> {
    recipes
        .iter()
        .find(|r| count >= r.input_count && r.input.matches(item, in_tag))
}

/// A Precision Sawmill recipe (`SawmillRecipe`): a main output, a secondary one with a chance, or both.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SawingRecipe {
    pub id: &'static str,
    pub input: Ingredient,
    pub input_count: u32,
    pub main: Option<(&'static str, u32)>,
    pub secondary: Option<(&'static str, u32)>,
    pub secondary_chance: f64,
}

/// A Combiner recipe (`CombinerRecipe`): a main input and an extra input make one output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CombiningRecipe {
    pub id: &'static str,
    pub main: Ingredient,
    pub main_count: u32,
    pub extra: Ingredient,
    pub extra_count: u32,
    pub output: &'static str,
    pub output_count: u32,
}

pub fn find_sawing(
    item: &str,
    count: u32,
    in_tag: &dyn Fn(&str, &str) -> bool,
) -> Option<&'static SawingRecipe> {
    crate::recipe_table::SAWING
        .iter()
        .find(|r| count >= r.input_count && r.input.matches(item, in_tag))
}

/// The first combining recipe for this main input and extra input (each with enough of it).
pub fn find_combining(
    main: (&str, u32),
    extra: (&str, u32),
    in_tag: &dyn Fn(&str, &str) -> bool,
) -> Option<&'static CombiningRecipe> {
    crate::recipe_table::COMBINING.iter().find(|r| {
        main.1 >= r.main_count
            && extra.1 >= r.extra_count
            && r.main.matches(main.0, in_tag)
            && r.extra.matches(extra.0, in_tag)
    })
}

/// An item-to-chemical recipe (`ItemStackToChemicalRecipe`): the Chemical Oxidizer's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChemicalRecipe {
    pub id: &'static str,
    pub input: Ingredient,
    pub input_count: u32,
    pub output: &'static str,
    pub amount: u64,
}

pub fn find_oxidizing(
    item: &str,
    count: u32,
    in_tag: &dyn Fn(&str, &str) -> bool,
) -> Option<&'static ChemicalRecipe> {
    crate::recipe_table::OXIDIZING
        .iter()
        .find(|r| count >= r.input_count && r.input.matches(item, in_tag))
}

/// An item and a chemical used a little each tick (`per_tick_usage`) make an item: purifying.
#[derive(Clone, Copy, Debug)]
pub struct ItemChemicalRecipe {
    pub id: &'static str,
    pub input: Ingredient,
    pub input_count: u32,
    pub chemical: &'static str,
    /// mB used each tick of the operation.
    pub chemical_amount: u64,
    pub output: &'static str,
    pub output_count: u32,
}

pub fn find_purifying(
    item: &str,
    count: u32,
    in_tag: &dyn Fn(&str, &str) -> bool,
) -> Option<&'static ItemChemicalRecipe> {
    crate::recipe_table::PURIFYING
        .iter()
        .find(|r| count >= r.input_count && r.input.matches(item, in_tag))
}

/// `metallurgic_infusing`: an item and the infuse chemical in the tank (used at the end of the
/// operation). The same item can infuse with several chemicals (dirt: fungi → mycelium, bio → podzol).
pub fn find_infusing(
    item: &str,
    count: u32,
    chemical: &str,
    in_tag: &dyn Fn(&str, &str) -> bool,
) -> Option<&'static ItemChemicalRecipe> {
    crate::recipe_table::METALLURGIC_INFUSING
        .iter()
        .find(|r| r.chemical == chemical && count >= r.input_count && r.input.matches(item, in_tag))
}

/// `chemical_conversion`: an item put in a machine's chemical slot turns into chemical (coal → 10
/// carbon).
pub fn find_conversion(
    item: &str,
    in_tag: &dyn Fn(&str, &str) -> bool,
) -> Option<&'static ChemicalRecipe> {
    crate::recipe_table::CHEMICAL_CONVERSION
        .iter()
        .find(|r| r.input.matches(item, in_tag))
}

/// The recipe types this port runs, with their tables.
pub fn table(kind: &str) -> &'static [ItemRecipe] {
    match kind {
        "enriching" => crate::recipe_table::ENRICHING,
        "crushing" => crate::recipe_table::CRUSHING,
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(tag: &str, item: &str) -> bool {
        tag == "c:ores/osmium" && item == "mekanism:osmium_ore"
    }

    #[test]
    fn osmium_ore_enriches_into_two_dusts() {
        let r = find(table("enriching"), "mekanism:osmium_ore", 1, &tags).expect("a recipe");
        assert_eq!((r.output, r.output_count), ("mekanism:dust_osmium", 2));
    }

    #[test]
    fn too_few_items_or_an_unknown_item_find_nothing() {
        assert!(find(table("enriching"), "minecraft:dirt", 64, &tags).is_none());
        let custom = [ItemRecipe {
            id: "t:x",
            input: Ingredient::Item("a"),
            input_count: 3,
            output: "b",
            output_count: 1,
        }];
        assert!(find(&custom, "a", 2, &tags).is_none());
        assert!(find(&custom, "a", 3, &tags).is_some());
    }

    #[test]
    fn the_tables_hold_the_generated_recipes() {
        assert!(
            table("enriching").len() > 50,
            "{}",
            table("enriching").len()
        );
        assert!(table("crushing").len() > 50, "{}", table("crushing").len());
    }

    #[test]
    fn osmium_ore_purifies_into_three_clumps_with_oxygen() {
        let r = find_purifying("mekanism:osmium_ore", 1, &tags).expect("a recipe");
        assert_eq!((r.output, r.output_count), ("mekanism:clump_osmium", 3));
        assert_eq!((r.chemical, r.chemical_amount), ("mekanism:oxygen", 1));
    }

    #[test]
    fn coal_gives_carbon_and_steel_takes_it() {
        let none = |_: &str, _: &str| false;
        let c = find_conversion("minecraft:coal", &|t, i| {
            t == "c:coal" && i == "minecraft:coal"
        })
        .expect("coal");
        assert_eq!(c.output, "mekanism:carbon");
        let iron = |t: &str, i: &str| t == "c:ingots/iron" && i == "minecraft:iron_ingot";
        let r = find_infusing("minecraft:iron_ingot", 1, "mekanism:carbon", &iron)
            .expect("enriched iron");
        assert_eq!(r.output, "mekanism:enriched_iron");
        let dirt = |chemical| find_infusing("minecraft:dirt", 1, chemical, &none).map(|r| r.output);
        assert_eq!(dirt("mekanism:fungi"), Some("minecraft:mycelium"));
        assert_eq!(dirt("mekanism:bio"), Some("minecraft:podzol"));
        assert_eq!(dirt("mekanism:carbon"), None);
    }
}
