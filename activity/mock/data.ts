// Fake songs for the mock page. The titles and artwork come from DeetsMusic's public web demo.

import type { Track } from "../deetsmusic-protocol";

const art = (path: string): string => `https://is1-ssl.mzstatic.com/image/thumb/${path}/{w}x{h}bb.jpg`;

export const SONGS: Track[] = [
  { catalogId: "1510728738", title: "Mẹẹn", artistName: "Joe Hertz & Noné", albumName: "Current Blues", durationMs: 180_000, artworkTemplate: art("Music113/v4/bc/3b/bc/bc3bbc9a-39b1-4f60-c8c8-af6d758522f5/5056167123781_1.jpg") },
  { catalogId: "1440618383", title: "Stuff We Did", artistName: "Michael Giacchino", albumName: "Up", durationMs: 136_000, artworkTemplate: art("Music114/v4/03/a1/7b/03a17b02-ec6b-ef7c-1558-b81ea2801139/00050087145095.rgb.jpg") },
  { catalogId: "6819812634", title: "Oh Medaase!", artistName: "El Michels Affair & Ahemaa Nwomkro", durationMs: 201_000, artworkTemplate: art("Music221/v4/74/68/ae/7468ae12-56f5-4f67-191f-5bca170004c9/69388.jpg") },
  { catalogId: "1557748162", title: "Crazy He Calls Me", artistName: "Conor Albert & Alice Auer", durationMs: 174_000, artworkTemplate: art("Music125/v4/ac/1b/3f/ac1b3fcd-15f1-f60d-ad16-e0de0252e028/cover.jpg") },
  { catalogId: "1734899229", title: "I Can’t Even Cry", artistName: "Hope Tala", durationMs: 189_000, artworkTemplate: art("Music211/v4/b2/ee/e4/b2eee4d9-9c89-71ad-0727-c8c818fd7b9d/49211.jpg") },
  { catalogId: "1833523527", title: "Winter Citrus (feat. Miya Folick)", artistName: "Julian Glander", durationMs: 163_000, artworkTemplate: art("Music211/v4/cd/e4/4e/cde44eeb-5e58-59bd-488c-51977642e6b0/199800380075.jpg") },
  { catalogId: "1566430748", title: "lime ocean paradise", artistName: "slenderbodies", durationMs: 214_000, artworkTemplate: art("Music125/v4/cf/25/9a/cf259a7c-55d6-4467-e511-0e73144d13f5/21UMGIM39927.rgb.jpg") },
  { catalogId: "6805707171", title: "SMALL TOWN", artistName: "Dominic Fike, Gabriel Jacoby & Emma Ogier", contentRating: "explicit", durationMs: 152_000, artworkTemplate: art("Music211/v4/81/24/65/812465c4-b20f-af62-4303-59a120c4199f/196874725467.jpg") },
  { catalogId: "6801692301", title: "You're Mine", artistName: "Anderson .Paak & Cordae", contentRating: "explicit", durationMs: 197_000, artworkTemplate: art("Music211/v4/2b/81/23/2b8123af-f191-32ea-e856-fc4ad33599b6/199316732108_cover.jpg") },
  { catalogId: "1534801763", title: "A Waltz For a Night", artistName: "Julie Delpy", durationMs: 208_000, artworkTemplate: art("Music114/v4/1e/13/e1/1e13e142-f338-3b42-19db-f6c48a8b26be/886448328575.jpg") },
  { catalogId: "1797750637", title: "SMOKE THE PAIN AWAY", artistName: "Calvin Harris", durationMs: 221_000, artworkTemplate: art("Music221/v4/0e/e5/26/0ee5261a-7dad-d6ff-b69d-bfb29bc4ab18/196872883374.jpg") },
  { catalogId: "1747393657", title: "California Winter", artistName: "Dhruv", durationMs: 183_000, artworkTemplate: art("Music221/v4/a8/93/73/a89373a4-f13a-d9d4-4b27-ab82e129d52a/196871771528.jpg") },
  { catalogId: "1200868888", title: "Keep Me", artistName: "Khalid", durationMs: 262_000, artworkTemplate: art("Music125/v4/f8/45/5a/f8455a71-8307-aa9a-9c95-3d22efe0804f/886446326146.jpg") },
  { catalogId: "6825024028", title: "'Some Days Are Made for Rain' (Music Inspired by LAIKA's Wildwood)", artistName: "Saya Gray & Wildwood Sessions", durationMs: 246_000, artworkTemplate: art("Music211/v4/8d/a0/ad/8da0ade7-f31e-ef3c-dd41-4171c90aa5f3/artwork.jpg") },
  { catalogId: "6793256485", title: "Shop Girl", artistName: "Kate Bollinger", durationMs: 171_000, artworkTemplate: art("Music221/v4/b4/52/61/b45261fd-df30-9a10-f0d4-b6c9051f8b15/67717.jpg") },
  { catalogId: "6786505387", title: "Summer Nights", artistName: "Zimmer90", durationMs: 195_000, artworkTemplate: art("Music211/v4/1b/16/1d/1b161d19-f07b-f95f-bf55-676c1b4e8231/820233868006.jpg") },
  { catalogId: "1756793124", title: "SOMETIMES I NEED TO YELL", artistName: "EVAN GIIA", durationMs: 178_000, artworkTemplate: art("Music221/v4/1f/79/1d/1f791d15-383c-f631-acf3-920423f715bd/193436391368_SABRINAKAUNEEGALBUMCOVERFINAL.jpg") },
];
