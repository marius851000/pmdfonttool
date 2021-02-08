use anyhow::{Context, Result};
use clap::Clap;
use image::{DynamicImage, GenericImage, ImageBuffer, Rgba};
use pmd_cte::{CteFormat, CteImage};
use pmd_dic::{KandChar, KandFile};
use std::{fs::{File, create_dir_all, read_dir}, path::PathBuf};
use std::str::FromStr;

#[derive(Clap)]
struct Opts {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Clap)]
enum SubCommand {
    /// Read a .dic file and a .img file, and generate a folder containing all the glyth
    Generate(GenerateParameter),
    /// Read a folder in the format extracted by generate, and write it to a .dic and a .img
    Build(BuildParameter),
}
#[derive(Clap)]
pub struct GenerateParameter {
    /// the input .dic file
    dic_input: PathBuf,
    /// the input .img file
    img_input: PathBuf,
    /// the output folder
    output: PathBuf,
}

#[derive(Clap)]
pub struct BuildParameter {
    /// the input folder
    input: PathBuf,
    /// the output .dic file
    dic_output: PathBuf,
    /// the output .img file
    img_output: PathBuf,
}

fn main() -> Result<()> {
    let opts = Opts::parse();
    match opts.subcmd {
        SubCommand::Generate(gp) => generate(gp).context("can't generate the glyphs")?,
        SubCommand::Build(bp) => build(bp).context("can't build the .img and .dic file")?,
    };
    Ok(())
}

fn generate(gp: GenerateParameter) -> Result<()> {
    println!("generating the editable font into {:?}", &gp.output);
    let mut input_kand = File::open(&gp.dic_input)?;
    let kand = KandFile::new_from_reader(&mut input_kand)?;
    let mut input_cte = File::open(&gp.img_input)?;
    let mut cte = CteImage::decode_cte(&mut input_cte)?;
    create_dir_all(&gp.output)?;
    for char in kand.chars {
        //TODO: this could panic
        let section = cte.image.crop(
            char.start_x as u32,
            char.start_y as u32,
            char.glyth_width as u32,
            char.glyth_height as u32,
        );
        let file_name = format!(
            "{}_{}_{}_{}_{}_{}.png",
            char.char, char.unk1, char.unk2, char.unk3, char.unk4, char.unk5
        );
        let target_file = gp.output.join(file_name);
        section.save(target_file)?;
    }
    println!("done");
    Ok(())
}

fn build(bp: BuildParameter) -> Result<()> {
    // TODO: start message
    // 1: read the input
    let mut atlas_width = 512;
    let mut chars = Vec::new();
    let mut max_width = 0;
    let mut lower_y = 0;
    let mut pos_x = 0;
    let mut pos_y = 0;
    for char_file_maybe in read_dir(&bp.input)? {
        let char_file = char_file_maybe?;
        let char_path = char_file.path();
        let stem = char_path
            .file_stem()
            .with_context(|| format!("the file at {:?} doesn't have a valid name", char_path))?
            .to_string_lossy();
        let mut splited = stem.split('_');

        let mut read_from_splited_u16 = || -> Result<u16> {
            let section = splited.next().with_context(|| format!("the path {:?} doesn't have the good format of \"charid_unk1_unk2_unk3_unk4_unk5.ext\"", char_path))?;
            Ok(u16::from_str(section).with_context(|| {
                format!(
                    "can't transform the text {:?} to a u16 character (for the file name at {:?})",
                    section, char_path
                )
            })?)
        };

        let char_id = read_from_splited_u16()?;
        let unk1 = read_from_splited_u16()?;
        let unk2 = read_from_splited_u16()?;
        let unk3 = read_from_splited_u16()?;
        let unk4 = read_from_splited_u16()?;
        let unk5 = read_from_splited_u16()?;

        let char_image = image::io::Reader::open(char_path)?.decode()?.to_rgba8();
        //TODO: what if they can't be transformed to as u16 ?
        let glyth_width = char_image.width() as u16;
        let glyth_height = char_image.height() as u16;
        // also, place the char
        let x_at_end_of_char = pos_x + glyth_width;
        if x_at_end_of_char >= atlas_width {
            pos_x = 0;
            pos_y = lower_y;
        };
        let start_x = pos_x;
        let start_y = pos_y;
        lower_y = lower_y.max(pos_y + glyth_height);
        pos_x += glyth_width;
        max_width = max_width.max(glyth_width);
        let char = KandChar {
            char: char_id,
            start_x,
            start_y,
            glyth_width,
            glyth_height,
            unk1,
            unk2,
            unk3,
            unk4,
            unk5,
        };
        chars.push((char, char_image));
    }
    // 2. create the atlas
    atlas_width = ((atlas_width.max(max_width) - 1) / 8 + 1) * 8;
    lower_y = ((lower_y - 1) / 8 + 1) * 8;
    let mut atlas: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::new(atlas_width as u32, lower_y as u32);
    for (char_data, char_image) in &chars {
        println!("{}, {}", char_data.start_x, char_data.start_y);
        atlas.copy_from(
            char_image,
            char_data.start_x as u32,
            char_data.start_y as u32,
        )?;
    }
    atlas.save("./test.png")?;

    // 3. save it
    let kand_file = KandFile {
        unk1: 0,
        unk2: 0,
        chars: chars.into_iter().map(|e| e.0).collect(),
    };
    let mut kand_writer = File::create(bp.dic_output)?;
    kand_file.write(&mut kand_writer)?;

    let cte_image = CteImage {
        original_format: CteFormat::A8,
        image: DynamicImage::ImageRgba8(atlas),
    };

    let mut cte_writer = File::create(bp.img_output)?;
    cte_image.encode_cte(&mut cte_writer)?;
    println!("done");
    Ok(())
}