use std::borrow::Cow;
use std::collections::HashMap;
use std::io::Write;
use flecs_ecs::macros::Component;
use slotmap::{new_key_type, SecondaryMap, SlotMap};
use valence_protocol::{ident, Encode, Ident, ItemKind, ItemStack, Packet};

/// Represents a packet sent from the server to the client to synchronize recipes.
#[derive(Clone, Debug, Encode, Packet)]
pub struct SynchronizeRecipesS2c<'a> {
    /// The list of recipes to synchronize.
    pub recipes: Vec<Recipe>,
}

/// Represents a single recipe in the Minecraft game.
#[derive(Clone, Debug, Encode)]
pub struct Recipe {
    /// The type of the recipe.
    pub kind: String,
    /// The unique identifier for this recipe.
    pub recipe_id: String,
    /// The specific data for this recipe, depending on its type.
    pub data: RecipeData,
}

struct RecipeIdentifier {
    kind: String,
    id: String,
}

/// Represents the different types of recipe data.
#[derive(Clone, Debug)]
pub enum RecipeData {
    CraftingShapeless(CraftingShapelessData),
    // CraftingShaped(CraftingShapedData<'a>),
    // CraftingSpecialArmordye(CraftingSpecialData),
    // CraftingSpecialBookcloning(CraftingSpecialData),
    // CraftingSpecialMapcloning(CraftingSpecialData),
    // CraftingSpecialMapextending(CraftingSpecialData),
    // CraftingSpecialFireworkRocket(CraftingSpecialData),
    // CraftingSpecialFireworkStar(CraftingSpecialData),
    // CraftingSpecialFireworkStarFade(CraftingSpecialData),
    // CraftingSpecialRepairitem(CraftingSpecialData),
    // CraftingSpecialTippedarrow(CraftingSpecialData),
    // CraftingSpecialBannerduplicate(CraftingSpecialData),
    // CraftingSpecialShielddecoration(CraftingSpecialData),
    // CraftingSpecialShulkerboxcoloring(CraftingSpecialData),
    // CraftingSpecialSuspiciousstew(CraftingSpecialData),
    // CraftingDecoratedPot(CraftingSpecialData),
    // Smelting(SmeltingData<'a>),
    // Blasting(SmeltingData<'a>),
    // Smoking(SmeltingData<'a>),
    // CampfireCooking(SmeltingData<'a>),
    // Stonecutting(StonecuttingData<'a>),
    // SmithingTransform(SmithingTransformData<'a>),
    // SmithingTrim(SmithingTrimData<'a>),
}

impl Encode for RecipeData {
    fn encode(&self, w: impl Write) -> anyhow::Result<()> {
        match self {
            RecipeData::CraftingShapeless(data) => data.encode(w),
            // RecipeData::CraftingShaped(data) => data.encode(w),
            // RecipeData::CraftingSpecialArmordye(data) => data.encode(w),
            // RecipeData::CraftingSpecialBookcloning(data) => data.encode(w),
            // RecipeData::CraftingSpecialMapcloning(data) => data.encode(w),
            // RecipeData::CraftingSpecialMapextending(data) => data.encode(w),
            // RecipeData::CraftingSpecialFireworkRocket(data) => data.encode(w),
            // RecipeData::CraftingSpecialFireworkStar(data) => data.encode(w),
            // RecipeData::CraftingSpecialFireworkStarFade(data) => data.encode(w),
            // RecipeData::CraftingSpecialRepairitem(data) => data.encode(w),
            // RecipeData::CraftingSpecialTippedarrow(data) => data.encode(w),
            // RecipeData::CraftingSpecialBannerduplicate(data) => data.encode(w),
            // RecipeData::CraftingSpecialShielddecoration(data) => data.encode(w),
            // RecipeData::CraftingSpecialShulkerboxcoloring(data) => data.encode(w),
            // RecipeData::CraftingSpecialSuspiciousstew(data) => data.encode(w),
            // RecipeData::CraftingDecoratedPot(data) => data.encode(w),
            // RecipeData::Smelting(data) => data.encode(w),
            // RecipeData::Blasting(data) => data.encode(w),
            // RecipeData::Smoking(data) => data.encode(w),
            // RecipeData::CampfireCooking(data) => data.encode(w),
            // RecipeData::Stonecutting(data) => data.encode(w),
            // RecipeData::SmithingTransform(data) => data.encode(w),
            // RecipeData::SmithingTrim(data) => data.encode(w),
        }
    }
}

/// Represents data for a shapeless crafting recipe.
#[derive(Clone, Debug)]
pub struct CraftingShapelessData {
    /// Used to group similar recipes together in the recipe book.
    pub group: String,
    /// The category of the recipe.
    pub category: CraftingCategory,
    /// The list of ingredients for the recipe.
    pub ingredients: Vec<Ingredient>,
    /// The result of the crafting recipe.
    pub result: ItemStack,
}

// /// Represents data for a shaped crafting recipe.
// #[derive(Clone, Debug)]
// pub struct CraftingShapedData<'a> {
//     /// The width of the crafting grid.
//     pub width: u32,
//     /// The height of the crafting grid.
//     pub height: u32,
//     /// Used to group similar recipes together in the recipe book.
//     pub group: Cow<'a, str>,
//     /// The category of the recipe.
//     pub category: CraftingCategory,
//     /// The ingredients for the recipe, indexed by x + (y * width).
//     pub ingredients: Cow<'a, [Ingredient<'a>]>,
//     /// The result of the crafting recipe.
//     pub result: ItemStack,
//     /// Whether to show a notification when the recipe is added.
//     pub show_notification: bool,
// }
// 
/// Represents data for special crafting recipes.
#[derive(Clone, Debug)]
pub struct CraftingSpecialData {
    /// The category of the special crafting recipe.
    pub category: CraftingCategory,
}

// /// Represents data for smelting-type recipes (smelting, blasting, smoking, campfire cooking).
// #[derive(Clone, Debug)]
// pub struct SmeltingData<'a> {
//     /// Used to group similar recipes together in the recipe book.
//     pub group: Cow<'a, str>,
//     /// The category of the smelting recipe.
//     pub category: SmeltingCategory,
//     /// The ingredient for the smelting recipe.
//     pub ingredient: Ingredient<'a>,
//     /// The result of the smelting recipe.
//     pub result: ItemStack,
//     /// The amount of experience granted by this recipe.
//     pub experience: f32,
//     /// The time it takes to complete this recipe.
//     pub cooking_time: u32,
// }
// 
// /// Represents data for a stonecutting recipe.
// #[derive(Clone, Debug)]
// pub struct StonecuttingData<'a> {
//     /// Used to group similar recipes together in the recipe book.
//     pub group: Cow<'a, str>,
//     /// The ingredient for the stonecutting recipe.
//     pub ingredient: Ingredient<'a>,
//     /// The result of the stonecutting recipe.
//     pub result: ItemStack,
// }
// 
// /// Represents data for a smithing transform recipe.
// #[derive(Clone, Debug)]
// pub struct SmithingTransformData<'a> {
//     /// The smithing template.
//     pub template: Ingredient<'a>,
//     /// The base item.
//     pub base: Ingredient<'a>,
//     /// The additional ingredient.
//     pub addition: Ingredient<'a>,
//     /// The result of the smithing transform.
//     pub result: ItemStack,
// }
// 
// /// Represents data for a smithing trim recipe.
// #[derive(Clone, Debug)]
// pub struct SmithingTrimData<'a> {
//     /// The smithing template.
//     pub template: Ingredient<'a>,
//     /// The base item.
//     pub base: Ingredient<'a>,
//     /// The additional ingredient.
//     pub addition: Ingredient<'a>,
// }

/// Represents the categories for crafting recipes.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Encode)]
pub enum CraftingCategory {
    Building,
    Redstone,
    Equipment,
    Misc,
}

/// Represents the categories for smelting recipes.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Encode)]
pub enum SmeltingCategory {
    Food,
    Blocks,
    Misc,
}

/// Represents an ingredient in a recipe, which can be multiple possible items.
pub type Ingredient = Vec<ItemStack>;

// Implement Encode for all structs
impl Encode for CraftingShapelessData {
    fn encode(&self, mut w: impl Write) -> anyhow::Result<()> {
        self.group.encode(&mut w)?;
        self.category.encode(&mut w)?;
        self.ingredients.encode(&mut w)?;
        self.result.encode(w)
    }
}

// impl Encode for CraftingShapedData<'_> {
//     fn encode(&self, mut w: impl Write) -> anyhow::Result<()> {
//         self.width.encode(&mut w)?;
//         self.height.encode(&mut w)?;
//         self.group.encode(&mut w)?;
//         self.category.encode(&mut w)?;
//         self.ingredients.encode(&mut w)?;
//         self.result.encode(&mut w)?;
//         self.show_notification.encode(w)
//     }
// }

impl Encode for CraftingSpecialData {
    fn encode(&self, w: impl Write) -> anyhow::Result<()> {
        self.category.encode(w)
    }
}

// impl Encode for SmeltingData<'_> {
//     fn encode(&self, mut w: impl Write) -> anyhow::Result<()> {
//         self.group.encode(&mut w)?;
//         self.category.encode(&mut w)?;
//         self.ingredient.encode(&mut w)?;
//         self.result.encode(&mut w)?;
//         self.experience.encode(&mut w)?;
//         self.cooking_time.encode(w)
//     }
// }
// 
// impl Encode for StonecuttingData<'_> {
//     fn encode(&self, mut w: impl Write) -> anyhow::Result<()> {
//         self.group.encode(&mut w)?;
//         self.ingredient.encode(&mut w)?;
//         self.result.encode(w)
//     }
// }
// 
// impl Encode for SmithingTransformData<'_> {
//     fn encode(&self, mut w: impl Write) -> anyhow::Result<()> {
//         self.template.encode(&mut w)?;
//         self.base.encode(&mut w)?;
//         self.addition.encode(&mut w)?;
//         self.result.encode(w)
//     }
// }
// 
// impl Encode for SmithingTrimData<'_> {
//     fn encode(&self, mut w: impl Write) -> anyhow::Result<()> {
//         self.template.encode(&mut w)?;
//         self.base.encode(&mut w)?;
//         self.addition.encode(w)
//     }
// }


#[derive(Debug, Encode, Packet)]
pub struct UnlockRecipesS2c {
    pub action: Action,
    pub crafting_recipe_book: RecipeBookState,
    pub smelting_recipe_book: RecipeBookState,
    pub blast_furnace_recipe_book: RecipeBookState,
    pub smoker_recipe_book: RecipeBookState,
    pub recipe_ids_1: Vec<String>,
    pub recipe_ids_2: Vec<String>,
}

#[derive(Debug, Encode)]
pub enum Action {
    Init,
    Add,
    Remove,
}

#[derive(Debug, Encode)]
pub struct RecipeBookState {
    pub open: bool,
    pub filter_active: bool,
}

impl RecipeBookState {
    pub const FALSE: Self = Self {
        open: false,
        filter_active: false,
    };
}


// since 3x3 grid are max
type Crafting3x3 = heapless::Vec<ItemKind, 9>;
type Crafting2x2 = heapless::Vec<ItemKind, 4>;

#[derive(Debug, Hash, PartialEq, Eq)]
struct SortedItemList(Crafting3x3);

impl From<Crafting3x3> for SortedItemList {
    fn from(mut list: Crafting3x3) -> Self {
        list.sort_unstable();
        Self(list)
    }
}

impl FromIterator<ItemKind> for SortedItemList {
    fn from_iter<T: IntoIterator<Item=ItemKind>>(iter: T) -> Self {
        let list: Crafting3x3 = iter.into_iter().collect();
        Self::from(list)
    }
}


// Define a custom key type
new_key_type! { struct SortedItemId; }

#[derive(Component)]
struct CraftingRegistry {
    // changes when the registry is updated
    epoch: u64,
    
    shapeless_lookup: HashMap<SortedItemList, SortedItemId>,
    shapeless: SlotMap<SortedItemId, CraftingShapelessData>,
}

struct ShapelessRecipe<'a> {
    // recipe_id: &'a RecipeIdentifier,
    data: &'a CraftingShapelessData,
}

impl CraftingRegistry {
    fn mark_changed(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
    }
    
    pub fn packe
    
    
    pub fn get_shapeless(&self, input: impl IntoIterator<Item=ItemKind>) -> Option<ShapelessRecipe<'_>> {
        let list: SortedItemList = input.into_iter().collect();
        let id = self.shapeless_lookup.get(&list).copied()?;

        // let recipe_id = self.shapeless_ids.get(id).unwrap();
        let data = self.shapeless.get(id).unwrap();

        Some(ShapelessRecipe { data })
    }

    fn register_shapeless(&mut self, recipe_id: RecipeIdentifier, data: CraftingShapelessData) {
        let list: SortedItemList = data.ingredients.iter().flatten().map(|x| x.item).collect();

        let recipe_id = self.shapeless.insert(data);
        self.shapeless_lookup.insert(list, recipe_id);
        
        self.mark_changed();
    }

    pub fn get_result_2x2(&self, grid: Crafting2x2) -> Option<&ItemStack> {
        if let Some(shapeless) = self.get_shapeless(grid) {
            return Some(&shapeless.data.result);
        }

        None
    }
}








