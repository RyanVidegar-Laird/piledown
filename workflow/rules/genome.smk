rule star_genome_generate:
    input:
        fasta="results/reference/GRCh38.primary_assembly.genome.fa.gz",
        gtf="results/reference/gencode.v{release}.annotation.gtf.gz".format(
            release=config["gencode_release"]
        ),
    output:
        directory("results/reference/star_index"),
    log:
        "results/logs/star_genome_generate.log",
    threads: config["star"]["threads"]
    shell:
        """
        mkdir -p {output}
        STAR --runMode genomeGenerate \
            --runThreadN {threads} \
            --genomeDir {output} \
            --genomeFastaFiles {input.fasta} \
            --readFilesCommand zcat \
            --sjdbGTFfile {input.gtf} \
            --sjdbOverhang {config[star][sjdb_overhang]} \
            2> {log}
        """
