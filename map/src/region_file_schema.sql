-- Metadata; currently, just the file version is here.
CREATE TABLE metadata (
	key varchar NOT NULL,
	value blob NOT NULL,  -- CBOR
);

-- 3D chunk data. This is the actual world data.
CREATE TABLE chunks (
	chunk_x int NOT NULL,
	chunk_y int NOT NULL,
	chunk_z int NOT NULL,
	compression int NOT NULL,
	chunk_data blob NOT NULL,
	PRIMARY KEY (chunk_x, chunk_y, chunk_z)
);

-- 2D topography data.
-- E.g., for the surface, this is roughly ground level, though note that chunk
-- generation might alter that slightly, to allow for overhangs, arches, etc.
--
-- This is only needed while generating chunks.
CREATE TABLE topography (
	topo_key_id int NOT NULL,
	topo_x int NOT NULL,
	topo_y int NOT NULL,
	compression int NOT NULL,
	topo_data blob NOT NULL,
);
