use anyhow::{Context, Result, anyhow};
use clap::Clap;
use fontdue::{Font, FontSettings};
use image::{DynamicImage, GenericImage, ImageBuffer, ImageFormat, LumaA, Rgba};
use pmd_cte::{CteFormat, CteImage};
use pmd_dic::{KandChar, KandFile};
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::convert::TryInto;
use std::path::Path;
use std::{fs::DirBuilder, io::Read, str::FromStr};
use std::{
    fs::{create_dir_all, read_dir, File},
    path::PathBuf,
    u16,
};

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
    /// Read a truetype font, and export a folder that can be read by the build command
    FromTruetype(FromTruetypeParameter),
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

#[derive(Clap)]
pub struct FromTruetypeParameter {
    /// a file listing every character to be exported (UTF-8, can be present multiple time)
    char_list_path: PathBuf,
    /// the input TrueType font
    input: PathBuf,
    /// the output folder
    output: PathBuf,
    /// the height of the generated font
    #[clap(default_value = "18")]
    scale: u16,
}

fn main() -> Result<()> {
    let opts = Opts::parse();
    match opts.subcmd {
        SubCommand::Generate(gp) => generate(gp).context("can't generate the glyphs")?,
        SubCommand::Build(bp) => build(bp).context("can't build the .img and .dic file")?,
        SubCommand::FromTruetype(fp) => {
            from_truetype(fp).context("can't generate the font result from the TrueType font")?
        }
    };
    Ok(())
}

pub struct CharData {
    pub glyth_width: u16,
    pub glyth_height: u16,
    pub xalign: i16,
    pub yalign: i16,
    pub distance: u16,
    pub unk4: u16,
    pub unk5: u16,
    pub image: ImageBuffer<Rgba<u8>, Vec<u8>>,
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
            char.char, char.unk1, char.unk2, char.distance, char.unk4, char.unk5
        );
        let target_file = gp.output.join(file_name);
        section.save(target_file)?;
    }
    println!("done");
    Ok(())
}

fn build(bp: BuildParameter) -> Result<()> {
    println!("starting the generation of {:?} and {:?}", bp.dic_output, bp.img_output);
    // 1: read the input
    let mut chars_data = BTreeMap::new();
    for char_file_maybe in read_dir(&bp.input)? {
        let char_file = char_file_maybe?;
        let char_path = char_file.path();
        let stem = char_path
            .file_stem()
            .with_context(|| format!("the file at {:?} doesn't have a valid name", char_path))?
            .to_string_lossy();
        let mut splited = stem.split('_');

        fn get_text_from_file_name<'a>(iter: &mut impl Iterator<Item = &'a str>, file_path: &Path) -> Result<&'a str> {
            iter.next().with_context(|| format!("the path {:?} doesn't have the good format of \"charid_unk1_unk2_distance_unk4_unk5.ext\"", file_path))
        }

        fn read_u16_from_splited<'a>(iter: &mut impl Iterator<Item = &'a str>, file_path: &Path) -> Result<u16>{
            let text = get_text_from_file_name(iter, file_path)?;
            Ok(u16::from_str(text).with_context(|| {
                format!(
                    "can't transform the text {:?} to a u16 number (for the file name at {:?})",
                    text, file_path
                )
            })?)
        }

        fn read_i16_from_splited<'a>(iter: &mut impl Iterator<Item = &'a str>, file_path: &Path) -> Result<i16>{
            let text = get_text_from_file_name(iter, file_path)?;
            Ok(i16::from_str(text).with_context(|| {
                format!(
                    "can't transform the text {:?} to a i16 number (for the file name at {:?})",
                    text, file_path
                )
            })?)
        }

        let char_id = read_u16_from_splited(&mut splited, &char_path)?;
        let unk1 = read_i16_from_splited(&mut splited, &char_path)?;
        let unk2 = read_i16_from_splited(&mut splited, &char_path)?;
        let distance = read_u16_from_splited(&mut splited, &char_path)?;
        let unk4 = read_u16_from_splited(&mut splited, &char_path)?;
        let unk5 = read_u16_from_splited(&mut splited, &char_path)?;

        let char_image = image::io::Reader::open(&char_path)?.decode()?.to_rgba8();
        let glyth_width = char_image.width().try_into().context("The width of the glyth is to big")?;
        let glyth_height = char_image.height().try_into().context("The height of the glyth is too big")?;
        if let Some(_) = chars_data.insert(char_id, CharData {
            glyth_height,
            glyth_width,
            xalign: unk1,
            yalign: unk2,
            distance,
            unk4,
            unk5,
            image: char_image,
        }) {
            return Err(anyhow!("The file {:?} represent a character that is also used by another file", char_path));
        }
    }
    
    // 2. create the atlas
    let mut atlas_width = 512;
    let mut chars = Vec::new();
    let mut max_width = 0;
    let mut lower_y = 0;
    let mut pos_x = 0;
    let mut pos_y = 0;

    for (char_id, char_data) in chars_data {
        // also, place the char
        let x_at_end_of_char = pos_x + char_data.glyth_width;
        if x_at_end_of_char >= atlas_width {
            pos_x = 0;
            pos_y = lower_y;
        };
        let start_x = pos_x;
        let start_y = pos_y;
        lower_y = lower_y.max(pos_y + char_data.glyth_height);
        pos_x += char_data.glyth_width;
        max_width = max_width.max(char_data.glyth_width);
        let char = KandChar {
            char: char_id,
            start_x,
            start_y,
            glyth_width: char_data.glyth_width,
            glyth_height: char_data.glyth_height,
            unk1: char_data.xalign,
            unk2: char_data.yalign,
            distance: char_data.distance,
            unk4: char_data.unk4,
            unk5: char_data.unk5,
        };
        chars.push((char, char_data.image));
    }

    atlas_width = ((atlas_width.max(max_width) - 1) / 8 + 1) * 8;
    lower_y = ((lower_y - 1) / 8 + 1) * 8;
    let mut atlas: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::new(atlas_width as u32, lower_y as u32);
    for (char_data, char_image) in &chars {
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

pub fn from_truetype(fp: FromTruetypeParameter) -> Result<()> {
    DirBuilder::new()
        .recursive(true)
        .create(&fp.output)
        .with_context(|| format!("can't create the target directory {:?}", fp.output))?;

    // list char to export
    let mut chars_to_include: BTreeSet<char> = BTreeSet::new();
    let mut char_list_file = File::open(&fp.char_list_path).context("can't open the file containing the list of char")?;
    let mut char_list_buffer = Vec::new();
    char_list_file.read_to_end(&mut char_list_buffer).context("can't read the file containing the char list")?;
    let char_list_string = String::from_utf8(char_list_buffer).context("The file contining the char list isn't a valid UTF-8 file")?;
    for chara in char_list_string.chars() {
        chars_to_include.insert(chara);
    };

    // read font
    let mut ttf_file =
        File::open(&fp.input).with_context(|| format!("can't open the file at {:?}", fp.input))?;
    let mut ttf_bytes = Vec::new();
    ttf_file.read_to_end(&mut ttf_bytes).with_context(|| {
        format!(
            "can't read the complete content of the file at {:?}",
            fp.input
        )
    })?;
    let ttf_font = Font::from_bytes(
        ttf_bytes,
        FontSettings {
            scale: fp.scale as f32,
            ..Default::default()
        },
    )
    .unwrap(); //TODO: make it work with anyhow

    for char in chars_to_include {
        println!("rasterizing {:?}", char);
        let (metric, bitmap_luminance) = ttf_font.rasterize(char, fp.scale as f32);
        let char_image: ImageBuffer<LumaA<u8>, Vec<_>> = if metric.width != 0 && metric.height != 0 {
            let mut bitmap: Vec<u8> = Vec::new();
            for pixel in bitmap_luminance.into_iter() {
                bitmap.push(0);
                bitmap.push(pixel);
            };
            ImageBuffer::from_vec(metric.width as u32, metric.height as u32, bitmap)
                    .with_context(|| format!("can't read the decoded character {:?}", char))?

        } else {
            ImageBuffer::new(1, 1)
        };
        //TODO: better parameter
        let file_name = format!("{}_{}_{}_{}_10_10.png", char as u16, metric.xmin as i16, -metric.ymin as i16 + fp.scale as i16 - metric.height as i16, metric.advance_width as i16);
        let mut out_char_path = fp.output.clone();
        out_char_path.push(file_name);
        char_image
            .save_with_format(&out_char_path, ImageFormat::Png)
            .with_context(|| format!("can't create/encode the image at {:?}", out_char_path))?;
    }

    Ok(())
}
